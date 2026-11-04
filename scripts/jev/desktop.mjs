/**
 * jev-desktop executor: the only part that speaks to agent-desktop. Every
 * observation and every mutation goes through this file, so the policy above it
 * never learns what a ref is.
 */
import { execFileSync } from "node:child_process";
import { existsSync } from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";
import { setTimeout as sleep } from "node:timers/promises";

import { collect, hasState, isMenuBar, isPopupHint, label, offerable, outsideWindow, overlayRole, popupHints } from "./screen.mjs";
import { ARGV } from "./policy.mjs";

const REPO = join(dirname(fileURLToPath(import.meta.url)), "..", "..");
const WINDOWS = process.platform === "win32";

export const bin =
  process.env.AGENT_DESKTOP_BIN ??
  (existsSync(join(REPO, "target/release/agent-desktop")) ? join(REPO, "target/release/agent-desktop") : "agent-desktop");

/**
 * JEV_TRACE=1 writes one line per binary call to stderr: the argv, the outcome
 * and how many refs came back. For a command that carries text, everything
 * after the command name and its ref is replaced by position, so a trace never
 * holds what was typed, copied or put back on the clipboard, whatever the text
 * begins with.
 */
const TEXT_COMMANDS = new Map([["set-value", 2], ["type", 2], ["clipboard-set", 1]]);

const countRefs = (node) => (node ? (node.ref_id ? 1 : 0) + (node.children ?? []).reduce((n, c) => n + countRefs(c), 0) : 0);

export const traced = (argv) => {
  const keep = TEXT_COMMANDS.get(argv[0]);
  return keep === undefined ? argv : argv.map((a, i) => (i < keep ? a : "[redacted]"));
};

const trace = (argv, result) => {
  if (process.env.JEV_TRACE !== "1") return result;
  const refs = result.data?.tree ? countRefs(result.data.tree) : undefined;
  process.stderr.write(`${JSON.stringify({ argv: traced(argv), ok: result.ok, code: result.error?.code, refs })}\n`);
  return result;
};

export const cli = (...argv) => {
  try {
    return trace(argv, JSON.parse(execFileSync(bin, argv, { encoding: "utf8", maxBuffer: 64 * 1024 * 1024 })));
  } catch (error) {
    try {
      return trace(argv, JSON.parse(error.stdout ?? ""));
    } catch {
      return trace(argv, { ok: false, error: { code: "SPAWN_FAILED", message: String(error.message ?? error) } });
    }
  }
};

/**
 * A cursor makes the run watchable: it travels to each element before the
 * operation lands. It is drawn where the element is, so the application has to
 * be in front of whatever else is on screen or the cursor arrives behind it.
 * Turning it on also starts the session that carries the trace.
 */
export const startCursor = (label) => {
  const session = cli("session", "start", "--name", "jev-desktop", "--cursor", "--multi-agent");
  if (!session.ok) return null;
  process.env.AGENT_DESKTOP_SESSION = session.data.session_id;
  process.env.AGENT_DESKTOP_AGENT_ID ??= "jev-desktop";
  cli("cursor-overlay", "enable", "--label", label.slice(0, 60));
  return session.data.session_id;
};

export const stopCursor = () => cli("cursor-overlay", "disable");

/**
 * On Windows, WinUI and XAML applications open menus, flyouts and combo lists
 * as a window of their own, which the binary's `menu` surface detector does not
 * cover. The owner's own tree still announces every open popup as a `window`
 * child named "Pop-up", and a rooted read of that child returns the popup's
 * items with refs that act, so a popup is read through its owner. Ownership is
 * given by the tree, a parent menu and its open submenu are both announced and
 * both are offered, and a read that answers for another window is discarded.
 * A popup that has just opened can read as an empty container until XAML has
 * populated it, so such a read is repeated a bounded number of times.
 */
const POPUP_READ_ATTEMPTS = 3;
const POPUP_SETTLE_MS = 200;
const CONTAINERS = new Set(["menu", "group", "pane", "window", "list", "listbox", "toolbar"]);

const settle = (ms) => Atomics.wait(new Int32Array(new SharedArrayBuffer(4)), 0, 0, ms);

const populated = (nodes) => nodes.some((n) => !CONTAINERS.has(n.role));

/** The items, plus a container only when it can scroll more of them into view. */
const actionable = (nodes) =>
  nodes.filter((n) => !isPopupHint(n) && (!CONTAINERS.has(n.role) || (n.available_actions ?? []).includes("Scroll")));

export const readPopups = (scope, home, hints, call = cli) => {
  const nodes = [];
  let count = 0;
  for (const hint of hints) {
    if (!hint.ref_id) continue;
    let found = [];
    for (let attempt = 0; attempt < POPUP_READ_ATTEMPTS; attempt += 1) {
      if (attempt) settle(POPUP_SETTLE_MS);
      const read = call("snapshot", ...scope, "--root", hint.ref_id, "-i", "--compact", "--include-bounds");
      if (!read.ok) break;
      if (home?.id && read.data.window?.id && read.data.window.id !== home.id) break;
      found = actionable(offerable(collect(read.data.tree)));
      if (populated(found)) break;
      found = [];
    }
    if (!found.length) continue;
    count += 1;
    nodes.push(...found);
  }
  return count ? { count, nodes } : null;
};

/**
 * A menu bar is the way into every menu and holds a handful of entries, so a
 * shallow read that cut it off is completed on the spot: its entries are
 * offered in its place. An entry is the element that opens the menu, which is
 * the entry itself when it advertises Click or Expand and otherwise the control
 * it wraps, so one name is one element and the opener is never dropped. A bar
 * that holds more stays a region for the policy to DRILL into, and a read that
 * answers for another window is discarded.
 */
const MENU_BAR_LIMIT = 12;
const OPENERS = ["Click", "Expand"];

const opens = (n) => (n.available_actions ?? []).some((a) => OPENERS.includes(a));

const entriesOf = (node, depth = 0) => {
  if (opens(node)) return [node];
  if (depth >= 2 || !node.children?.length) return [];
  if (node.role === "menuitem") {
    const wrapped = node.children.find((c) => opens(c));
    return wrapped ? [wrapped] : [];
  }
  return node.children.flatMap((c) => entriesOf(c, depth + 1));
};

export const expandMenuBars = (nodes, scope, home, call = cli) => {
  const out = [];
  for (const node of nodes) {
    if (!(isMenuBar(node) && node.children_count > 0 && node.children_count <= MENU_BAR_LIMIT)) {
      out.push(node);
      continue;
    }
    const bar = call("snapshot", ...scope, "--root", node.ref_id, "-i", "--compact", "--include-bounds");
    const foreign = home?.id && bar.ok && bar.data.window?.id && bar.data.window.id !== home.id;
    if (!bar.ok || foreign) {
      out.push(node);
      continue;
    }
    const entries = (bar.data.tree.children ?? []).flatMap((c) => entriesOf(c));
    const items = offerable(entries.map((n) => ({ ...n, path: [...node.path, label(node)], parentRole: node.role })));
    out.push(...(items.length ? items : [node]));
  }
  return out;
};

/**
 * A window is read shallowly until the policy asks to go deeper, so a document
 * holding thousands of elements costs the same first look as a panel holding
 * thirty. A region cut off by that shallow read still reports how much it holds,
 * which is what makes it worth drilling into. A sheet or menu owns the screen
 * while it is up, so it is read instead of the window behind it. On Windows an
 * open popup is announced by the owner's whole-window read; a rooted read shows
 * the entry that opened it in the `expanded` state and then costs one
 * whole-window read to find the announcement. The root is kept for afterwards.
 */
export const observe = (app, root, windowId = null) => {
  const scope = windowId ? ["--app", app, "--window-id", windowId] : ["--app", app];
  const base = ["snapshot", ...scope, "-i", "--compact", "--include-bounds"];
  const unreadable = (what, error) => Object.assign(new Error(`${what} could not be read: ${error?.code}`),
    { code: error?.code ?? null });
  let snap = root ? cli(...base, "--root", root) : cli(...base, "--skeleton");
  if (!snap.ok && root) throw unreadable("that region", snap.error);
  if (snap.ok && root && windowId && outsideWindow(snap.data, windowId)) {
    throw Object.assign(new Error(`that region is not in window ${windowId}`), { code: "ROOT_OUTSIDE_WINDOW" });
  }
  if (!snap.ok && snap.error?.code !== "WINDOW_NOT_FOUND") snap = cli(...base, "--max-depth", "4");
  if (!snap.ok) throw unreadable("the screen", snap.error);
  const home = snap.data.window ?? null;
  let surface = root ? null : overlayRole(snap.data.tree);
  if (surface) {
    const scoped = cli("snapshot", ...scope, "--surface", surface, "-i", "--compact", "--include-bounds");
    if (scoped.ok && windowId && outsideWindow(scoped.data, windowId)) {
      throw Object.assign(new Error(`the ${surface} is not in window ${windowId}`), { code: "SURFACE_OUTSIDE_WINDOW" });
    }
    if (scoped.ok) snap = scoped;
    else surface = null;
  }
  let popup = null;
  if (WINDOWS && !surface) {
    let hints = root ? [] : popupHints(snap.data.tree);
    if (root && hasState(snap.data.tree, "expanded")) {
      const whole = cli(...base, "--skeleton");
      if (whole.ok && (!windowId || !outsideWindow(whole.data, windowId))) hints = popupHints(whole.data.tree);
    }
    if (hints.length) {
      popup = readPopups(scope, home, hints);
      if (popup) surface = "popup";
    }
  }
  let nodes = popup ? popup.nodes : offerable(collect(snap.data.tree));
  if (!root && !surface) nodes = expandMenuBars(nodes, scope, home);
  return {
    nodes,
    screen: { app, window: snap.data.window?.title ?? null, window_id: snap.data.window?.id ?? windowId,
      surface: surface ?? "window", root, ...(popup ? { popups: popup.count } : {}) },
  };
};

/**
 * The clipboard is borrowed, never assumed. It is read only when a paste is
 * about to need it, so a run that never pastes leaves it untouched. What was
 * there is put back if it was text; if it held an image, a file, or nothing that
 * could be read, it is emptied instead, because leaving the run's own string
 * behind is worse than leaving it empty.
 */
export const clipboardGuard = () => {
  let held = null;
  return {
    borrow() {
      if (held) return;
      const read = cli("clipboard-get");
      held = typeof read.data?.text === "string" ? { text: read.data.text } : { unreadable: true };
    },
    restore() {
      if (!held) return;
      if (held.text === undefined) cli("clipboard-clear");
      else cli("clipboard-set", held.text);
      held = null;
    },
  };
};

const verifyWindowMs = (raw) => {
  const value = Number(raw ?? 2000);
  if (!Number.isFinite(value) || value < 0 || value > 60000) {
    throw new Error(`JEV_VERIFY_MS must be a number of milliseconds from 0 to 60000, not ${JSON.stringify(raw)}`);
  }
  return value;
};
const VERIFY_MS = verifyWindowMs(process.env.JEV_VERIFY_MS);
const VERIFY_INTERVAL_MS = 100;

const awaitFieldText = async (ref, text) => {
  const deadline = Date.now() + VERIFY_MS;
  for (;;) {
    const observed = cli("get", ref, "--property", "value");
    if (observed.ok && observed.data?.value === text) return { held: true, observed };
    if (Date.now() >= deadline) return { held: false, observed };
    await sleep(VERIFY_INTERVAL_MS);
  }
};

const verificationFailure = (observed) => ({
  code: "TEXT_VERIFICATION_FAILED",
  message: "after the paste the field does not hold the requested text",
  disposition: { delivery: "delivered_unverified", retry: "unsafe" },
  ...(observed.ok ? {} : { details: { read_error: observed.error ?? null } }),
});

/**
 * Text goes in through whichever route the application accepts. A direct value
 * write is one verified call, and the applications that refuse it report that
 * refusal, so the paste path runs only when it is needed. A paste arrives whole
 * where one key press per character loses characters and capitals.
 */
export const enterText = async ({ app, windowId = null }, node, text, clipboard) => {
  const written = cli("set-value", node.ref_id, text);
  if (written.ok) return { route: "set-value", result: written };
  const failure = written.error;
  if (failure && failure.code !== "SPAWN_FAILED" && !failure.disposition) {
    return { route: "set-value", result: { ok: false, error: {
      code: "BINARY_TOO_OLD",
      message: "this agent-desktop binary reports no error disposition; the paste fallback needs a binary whose failed set-value says whether a retry is safe",
      details: { cause: failure },
    } } };
  }
  if (failure?.disposition?.retry !== "safe") return { route: "set-value", result: written };
  const focused = cli("focus", node.ref_id);
  if (!focused.ok) return { route: "paste", result: focused };
  clipboard.borrow();
  const copied = cli("clipboard-set", text);
  if (!copied.ok) return { route: "paste", result: copied };
  const pasted = cli("press", "cmd+v", "--app", app, ...(windowId ? ["--window-id", windowId] : []));
  if (!pasted.ok) return { route: "paste", result: pasted };
  const { held, observed } = await awaitFieldText(node.ref_id, text);
  if (!held) return { route: "paste", result: { ok: false, error: verificationFailure(observed) } };
  return { route: "paste", result: { ...pasted, data: {
    ...pasted.data, disposition: { delivery: "delivered_verified", retry: "unsafe" },
  } } };
};

export const execute = async (target, operation, node, text, clipboard) => {
  if (operation === "WAIT") {
    await sleep(250);
    return { ok: true, delivery: "waited" };
  }
  if (operation === "DRILL") return { ok: true, delivery: "looked", root: node.ref_id };
  if (operation === "WIDEN") return { ok: true, delivery: "looked", root: null };
  if (operation === "TYPE_TEXT") {
    const { route, result } = await enterText(target, node, text, clipboard);
    return { ...deliveryEvidence(result), route };
  }
  const result = cli(...ARGV[operation](node.ref_id));
  return deliveryEvidence(result);
};

export const deliveryEvidence = (result) => {
  const details = result.data?.details ?? result.error?.details ?? null;
  const after = result.error?.details?.after_action;
  return {
    ok: result.ok,
    delivery: result.data?.disposition?.delivery ?? result.error?.disposition?.delivery ?? null,
    error: result.error ?? null,
    steps: result.data?.steps ?? after?.steps ?? [],
    post_state: result.data?.post_state ?? details?.post_state ?? after?.post_state ?? null,
    details,
  };
};
