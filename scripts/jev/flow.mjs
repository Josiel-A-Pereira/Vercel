#!/usr/bin/env node
/**
 * jev-desktop flow: several steps per screen instead of one.
 *
 *   node scripts/jev/flow.mjs --app msedge --window-id w-42 --execute \
 *     "the search box at the top of the page" --text "你好" press:return
 *
 * Each round reads the screen once, asks Jev for the target of every pending
 * step in a single request, and runs the leading steps it is sure of as one
 * agent-desktop batch. A step whose element is not on this screen yet stops
 * the batch there; the next round reads the new screen and asks again for
 * what is left. `--text` belongs to the step written before it, and
 * `press:<combo>` sends a key without asking Jev anything.
 *
 * Grouping decisions reduces the number of model requests. Total latency
 * still depends on the application, the request size, and the model service.
 */
import { fileURLToPath } from "node:url";

import { cli } from "./desktop.mjs";
import { snapshotFlags } from "./bin.mjs";
import { collect, offerable, outsideWindow, overlayRole } from "./screen.mjs";
import { NO_MATCH, fingerprint, resolveModel, route, typesafeApi } from "./policy.mjs";
import { reconcile } from "./act.mjs";

const CAP = 150;
const MAX_ROUNDS = 8;
const SETTLE_READS = 4;
const SETTLE_WAIT_MS = 300;
const FLOW_DEPTH = "30";

export const parseSteps = (argv) => {
  const steps = [];
  const flags = { execute: false };
  for (let i = 0; i < argv.length; i++) {
    const arg = argv[i];
    if (arg === "--text") {
      if (!steps.length || steps.at(-1).press || argv[i + 1] === undefined) {
        throw new Error("--text needs a value and must follow the step it belongs to");
      }
      steps.at(-1).text = argv[++i];
    } else if (arg === "--app" || arg === "--window-id") {
      if (argv[i + 1] === undefined) throw new Error(`${arg} needs a value`);
      flags[arg === "--app" ? "app" : "windowId"] = argv[++i];
    } else if (arg === "--execute") {
      flags.execute = true;
    } else if (arg.startsWith("--")) {
      throw new Error(`unknown flag ${arg}`);
    } else {
      steps.push(arg.startsWith("press:") ? { intent: arg, press: arg.slice(6) } : { intent: arg });
    }
  }
  return { steps: steps.map((step, index) => ({ ...step, index })), flags };
};

/** Offscreen and unlabeled elements cannot be told apart in a short description, so they stay out. */
export const candidates = (tree, cap = CAP) =>
  offerable(collect(tree))
    .filter((n) => !(n.states ?? []).includes("offscreen") && (n.name || n.description || n.value))
    .slice(0, cap);

/** Every question repeats the whole list, so each element costs a role, a label and a short value. */
const brief = (n) => ({
  what: `${n.role} "${String(n.name ?? n.description ?? "").slice(0, 60)}"`,
  ...(n.value ? { holds: String(n.value).slice(0, 60) } : {}),
});

export const buildFlowRequest = (steps, screen, nodes) => {
  const criteria = {
    ...Object.fromEntries(nodes.map((n) => [n.ref_id, brief(n)])),
    [NO_MATCH]: { what: "None of the above", when: "The element is not on this screen yet." },
  };
  const questions = {};
  for (const step of steps) {
    questions[`target_${step.index}`] = {
      type: "choice",
      instructions: {
        task: `Which element on screen does this step refer to: ${step.intent}`,
        how: "Match the step against each element's role, label and the value it holds.",
        note: "Judge identity only. Answer none when the element is not on this screen.",
      },
      criteria,
    };
    questions[`risk_${step.index}`] = {
      type: "noul",
      instructions: `Would carrying out this step be hard or impossible to undo: ${step.intent}`,
    };
  }
  return { state: { app: screen.app, window: screen.window, surface: screen.surface, element_count: nodes.length },
    model: resolveModel(), questions };
};

export const readFlowAnswers = (body, steps) =>
  Object.fromEntries(steps.map((step) => {
    const target = body?.answers?.[`target_${step.index}`];
    const risk = body?.answers?.[`risk_${step.index}`];
    return [step.index, target?.type === "choice"
      ? { target: target.choice, targetConfidence: target.confidence ?? 0, present: null, destructive: risk?.noul ?? null }
      : { target: NO_MATCH, targetConfidence: 0, present: null, destructive: null }];
  }));

const ARGS = {
  "set-value": (ref, text) => ({ ref_id: ref, value: text }),
  select: (ref, text) => ({ ref_id: ref, value: text }),
  type: (ref, text) => ({ ref_id: ref, text }),
  scroll: (ref) => ({ ref_id: ref, direction: "down" }),
};

export const toEntries = (step, node, { app, windowId }) => {
  if (step.press) return [{ command: "press", args: { combo: step.press, app, ...(windowId ? { window_id: windowId } : {}) } }];
  const hasText = step.text !== undefined;
  const { verb } = reconcile(hasText ? "set-value" : "click", node, hasText);
  return [{ command: verb, args: (ARGS[verb] ?? ((ref) => ({ ref_id: ref })))(node.ref_id, step.text) }];
};

/** The leading steps Jev is sure of run together; the first one it is not sure of ends the batch. */
export const plan = (pending, answers, nodes, target) => {
  const byRef = new Map(nodes.map((n) => [n.ref_id, n]));
  const entries = [];
  const decisions = [];
  for (const step of pending) {
    if (step.press) {
      entries.push(...toEntries(step, null, target).map((e) => ({ ...e, step: step.index })));
      decisions.push({ step: step.index, decision: "act", key: step.press });
      continue;
    }
    const answer = answers[step.index];
    const routed = route(answer);
    const node = byRef.get(answer.target);
    const label = node ? `${node.role} "${String(node.name ?? node.description ?? "").slice(0, 40)}"` : null;
    decisions.push({ step: step.index, decision: routed.decision, why: routed.why, element: label,
      confidence: answer.targetConfidence, destructive: answer.destructive });
    if (routed.decision !== "act" || !node) break;
    entries.push(...toEntries(step, node, target).map((e) => ({ ...e, step: step.index })));
  }
  return { entries, decisions };
};

const deepFlags = () => {
  const flags = snapshotFlags();
  return flags.includes("--max-depth") ? flags : [...flags, "--max-depth", FLOW_DEPTH];
};

/** One deep read per screen. An open sheet, menu or Windows popover is read instead of the window behind it. */
const readScreen = (app, windowId) => {
  const scope = windowId ? ["--app", app, "--window-id", windowId] : ["--app", app];
  const flags = deepFlags();
  let snap = cli("snapshot", ...scope, "-i", "--compact", ...flags);
  if (!snap.ok) throw Object.assign(new Error(`the screen could not be read: ${snap.error?.code}`), { code: snap.error?.code });
  let surface = overlayRole(snap.data.tree);
  const kind = surface ?? (process.platform === "win32" ? "popover" : null);
  if (kind) {
    const scoped = cli("snapshot", ...scope, "--surface", kind, "-i", "--compact", ...flags);
    if (scoped.ok && candidates(scoped.data.tree).length && !(windowId && outsideWindow(scoped.data, windowId))) {
      snap = scoped;
      surface = kind;
    } else if (!surface) {
      surface = null;
    }
  }
  return { snap, screen: { app, window: snap.data.window?.title ?? null, surface: surface ?? "window" } };
};

const ask = async (payload) => {
  for (let attempt = 0; ; attempt++) {
    const res = await fetch(typesafeApi(), {
      method: "POST",
      redirect: "error",
      headers: { Authorization: `Bearer ${process.env.TYPESAFE_API_KEY}`, "Content-Type": "application/json" },
      body: JSON.stringify(payload),
    }).catch(() => ({ ok: false, status: 0 }));
    if (res.ok) return res.json();
    if (attempt >= 2 || ![429, 503, 529].includes(res.status)) throw new Error(`decision endpoint returned HTTP ${res.status}`);
    await new Promise((resolve) => setTimeout(resolve, 500 * (attempt + 1)));
  }
};

const since = (start) => Math.round(performance.now() - start);
const emit = (event) => console.log(JSON.stringify(event));

/** The steps whose own action ran. A step owns one entry, so a failed entry fails only its step. */
export const completedSteps = (entries, results) =>
  new Set(entries.filter((entry, i) => results[i]?.ok).map((entry) => entry.step));

/**
 * After an action the screen is read until it differs from the one acted on, so
 * Jev is never asked about a page that has not reacted yet. A screen that never
 * changes is returned as it is, and the caller decides what that means.
 */
const settledScreen = async (app, windowId, actedOn) => {
  for (let read = 1; ; read++) {
    const seen = readScreen(app, windowId);
    const nodes = candidates(seen.snap.data.tree);
    const print = fingerprint(nodes);
    if (!actedOn || print !== actedOn || read >= SETTLE_READS) return { ...seen, nodes, print, changed: print !== actedOn };
    await new Promise((resolve) => setTimeout(resolve, SETTLE_WAIT_MS));
  }
};

export const flow = async (steps, { app, windowId = null, execute = false } = {}) => {
  let pending = steps;
  let actedOn = null;
  let idleOn = null;
  for (let round = 1; pending.length; round++) {
    if (round > MAX_ROUNDS) return { stop: "too many rounds", pending: pending.map((s) => s.index) };
    let start = performance.now();
    const { screen, nodes, print, changed } = await settledScreen(app, windowId, actedOn);
    const readMs = since(start);
    const asked = pending.filter((s) => !s.press);
    start = performance.now();
    const answers = asked.length && nodes.length ? readFlowAnswers(await ask(buildFlowRequest(asked, screen, nodes)), asked) : {};
    for (const step of asked) answers[step.index] ??= { target: NO_MATCH, targetConfidence: 0, present: null, destructive: null };
    const askMs = since(start);
    const { entries, decisions } = plan(pending, answers, nodes, { app, windowId });
    const event = { round, surface: screen.surface, candidates: nodes.length, read_ms: readMs, ask_ms: askMs, decisions };
    if (!execute) {
      emit({ ...event, planned: entries.map(({ step, ...e }) => ({ step, ...e })) });
      return { stop: "dry_run" };
    }
    if (!entries.length) {
      emit(event);
      if (actedOn && !changed) {
        return { stop: "no_effect", why: "the screen did not change after the previous step, so it may not have taken effect",
          step: decisions.at(-1) };
      }
      if (idleOn === print) return { stop: "blocked", step: decisions.at(-1) };
      idleOn = print;
      actedOn = print;
      continue;
    }
    idleOn = null;
    start = performance.now();
    const batch = cli("batch", "--stop-on-error", JSON.stringify(entries.map(({ step, ...e }) => e)));
    const results = batch.data?.results ?? [];
    const done = completedSteps(entries, results);
    const firstError = results.find((r) => !r.ok)?.error ?? (batch.ok ? null : batch.error);
    emit({ ...event, act_ms: since(start), ran: [...done], ...(firstError ? { error: firstError.code } : {}) });
    const refreshable = firstError?.code === "STALE_REF"
      && firstError.disposition?.retry === "safe"
      && firstError.disposition?.delivery === "not_delivered";
    if (firstError && !refreshable) {
      return { stop: "action_failed", error: firstError };
    }
    pending = pending.filter((s) => !done.has(s.index));
    actedOn = print;
  }
  return { done: true };
};

const main = async (argv) => {
  let parsed;
  try {
    parsed = parseSteps(argv);
  } catch (error) {
    console.error(String(error.message));
    process.exit(2);
  }
  const { steps, flags } = parsed;
  if (!flags.app || !steps.length) {
    console.error('usage: flow.mjs --app <name> [--window-id <id>] [--execute] "<step>" [--text "…"] [press:<combo>] …');
    process.exit(2);
  }
  if (!process.env.TYPESAFE_API_KEY) throw new Error("TYPESAFE_API_KEY unset");
  const start = performance.now();
  const result = await flow(steps, flags);
  emit({ ...result, total_ms: since(start) });
  process.exitCode = result.done || result.stop === "dry_run" ? 0 : 1;
};

if (process.argv[1] === fileURLToPath(import.meta.url)) {
  await main(process.argv.slice(2)).catch((error) => {
    console.log(JSON.stringify({ ok: false, error: String(error.message ?? error) }));
    process.exitCode = 1;
  });
}
