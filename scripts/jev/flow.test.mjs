import assert from "node:assert/strict";

import { buildFlowRequest, candidates, completedSteps, parseSteps, plan, readFlowAnswers, toEntries } from "./flow.mjs";

const parsed = parseSteps(["--app", "msedge", "--window-id", "w-1", "--execute",
  "the search box", "--text", "你好", "press:return", "the first result"]);
assert.equal(parsed.flags.app, "msedge");
assert.equal(parsed.flags.windowId, "w-1");
assert.equal(parsed.flags.execute, true);
assert.deepEqual(parsed.steps.map((s) => [s.index, s.intent, s.text ?? null, s.press ?? null]), [
  [0, "the search box", "你好", null],
  [1, "press:return", null, "return"],
  [2, "the first result", null, null],
]);
assert.throws(() => parseSteps(["--text", "x"]), /must follow/, "text with no step before it");
assert.throws(() => parseSteps(["press:return", "--text", "x"]), /must follow/, "a key press takes no text");
assert.throws(() => parseSteps(["--bogus"]), /unknown flag/);

const S = "s1";
const tree = {
  role: "window",
  children: [
    { ref_id: `@${S}:e1`, role: "textfield", name: "搜索", available_actions: ["SetValue", "Click"] },
    { ref_id: `@${S}:e2`, role: "button", name: "搜索", available_actions: ["Click"] },
    { ref_id: `@${S}:e3`, role: "link", name: "Below", states: ["offscreen"], available_actions: ["Click"] },
    { ref_id: `@${S}:e4`, role: "group", available_actions: ["Click"] },
    { ref_id: `@${S}:e5`, role: "button", name: "Gone", states: ["disabled"], available_actions: ["Click"] },
  ],
};
const nodes = candidates(tree);
assert.deepEqual(nodes.map((n) => n.ref_id), [`@${S}:e1`, `@${S}:e2`],
  "offscreen, unlabeled and disabled elements are never offered");
assert.equal(candidates(tree, 1).length, 1, "the cap bounds the request");

const asked = parsed.steps.filter((s) => !s.press);
const request = buildFlowRequest(asked, { app: "msedge", window: "B", surface: "window" }, nodes);
assert.deepEqual(Object.keys(request.questions), ["target_0", "risk_0", "target_2", "risk_2"],
  "one request carries a target and a risk question for every step that needs an element");
assert.deepEqual(Object.keys(request.questions.target_0.criteria), [`@${S}:e1`, `@${S}:e2`, "none"]);
assert.deepEqual(request.questions.target_0.criteria[`@${S}:e1`], { what: 'textfield "搜索"' });

const answers = readFlowAnswers({ answers: {
  target_0: { type: "choice", choice: `@${S}:e1`, confidence: 0.95 },
  risk_0: { type: "noul", noul: 0.1 },
  target_2: { type: "choice", choice: "none", confidence: 0.9 },
} }, asked);
assert.equal(answers[0].target, `@${S}:e1`);
assert.equal(answers[0].destructive, 0.1);
assert.equal(answers[2].target, "none");
assert.deepEqual(readFlowAnswers({}, asked)[0].target, "none", "a missing answer is no match, never a guess");

const target = { app: "msedge", windowId: "w-1" };
const planned = plan(parsed.steps, answers, nodes, target);
assert.deepEqual(planned.entries.map((e) => [e.step, e.command]), [[0, "set-value"], [1, "press"]],
  "the steps Jev is sure of run together and the first unsure one ends the batch");
assert.deepEqual(planned.entries[0].args, { ref_id: `@${S}:e1`, value: "你好" });
assert.deepEqual(planned.entries[1].args, { combo: "return", app: "msedge", window_id: "w-1" });
assert.ok(!planned.entries.some((e) => e.command === "focus"),
  "no separate focus: on Windows it moves the foreground and headless delivery refuses it");
assert.deepEqual(planned.decisions.map((d) => [d.step, d.decision]), [[0, "act"], [1, "act"], [2, "abstain"]]);

const unsure = plan(parsed.steps, { ...answers, 0: { ...answers[0], targetConfidence: 0.6 } }, nodes, target);
assert.equal(unsure.entries.length, 0, "a step under the bar runs nothing after it either");
assert.equal(unsure.decisions[0].decision, "confirm");

const risky = plan(parsed.steps, { ...answers, 0: { ...answers[0], targetConfidence: 0.8, destructive: 0.9 } }, nodes, target);
assert.equal(risky.entries.length, 0, "a hard-to-undo step needs the higher bar");

assert.deepEqual(toEntries({ intent: "press the search button" }, nodes[1], target),
  [{ command: "click", args: { ref_id: `@${S}:e2` } }]);
assert.deepEqual(toEntries({ intent: "x", text: "y" }, { ref_id: "@s1:e9", available_actions: ["TypeText"] }, target),
  [{ command: "type", args: { ref_id: "@s1:e9", text: "y" } }], "a field without SetValue is typed into");

const twoSteps = [{ step: 0, command: "set-value" }, { step: 1, command: "press" }];
assert.deepEqual([...completedSteps(twoSteps, [{ ok: true }, { ok: true }])], [0, 1]);
assert.deepEqual([...completedSteps(twoSteps, [{ ok: true }, { ok: false }])], [0],
  "a failed key press fails only its own step; the value already written stays done");
assert.deepEqual([...completedSteps(twoSteps, [{ ok: true }])], [0], "an entry the batch never started is not done");

console.log("ok");
