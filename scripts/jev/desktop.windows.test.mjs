import assert from "node:assert/strict";
import { test } from "node:test";

import { expandMenuBars, readPopups, traced } from "./desktop.mjs";
import { hasState, overlayRole, popupHints } from "./screen.mjs";

/** A binary that answers from a table keyed by the start of the argv, and counts what it was asked. */
const binary = (answers) => {
  const calls = [];
  const call = (...argv) => {
    calls.push(argv);
    const key = argv.join(" ");
    for (const [prefix, answer] of answers) {
      if (key.startsWith(prefix)) return typeof answer === "function" ? answer(calls.length) : answer;
    }
    return { ok: false, error: { code: "SPAWN_FAILED", message: key } };
  };
  call.calls = calls;
  return call;
};

const item = (name, actions = ["Click", "ScrollTo", "SetFocus"]) =>
  ({ ref_id: `@s1:${name}`, role: "menuitem", name, available_actions: actions });
const hint = (id) => ({ ref_id: `@s1:${id}`, role: "window", name: "Pop-up", states: ["offscreen", "modal"], children_count: 1, available_actions: ["ScrollTo"] });
const popupRead = (windowId, children, containerActions = ["ScrollTo", "SetFocus"]) => ({ ok: true, data: { window: { id: windowId, title: "Untitled - Notepad" }, tree: {
  ...hint("self"), children: [{ role: "menu", available_actions: containerActions, ref_id: "@s1:menu", children }],
} } });

const scope = ["--app", "Notepad", "--window-id", "w-a"];
const home = { id: "w-a", title: "Untitled - Notepad" };
const names = (nodes) => nodes.map((n) => n.name);

test("popups are read through the owner's own Pop-up children, so a parent menu and its submenu are both offered", () => {
  const call = binary([
    ["snapshot --app Notepad --window-id w-a --root @s1:h1", popupRead("w-a", [item("Zoom in"), item("Zoom out")])],
    ["snapshot --app Notepad --window-id w-a --root @s1:h2", popupRead("w-a", [item("Zoom", ["Expand", "Click"]), item("Status bar")])],
  ]);
  const read = readPopups(scope, home, [hint("h1"), hint("h2")], call);
  assert.equal(read.count, 2);
  assert.deepEqual(names(read.nodes), ["Zoom in", "Zoom out", "Zoom", "Status bar"]);
  assert.ok(read.nodes.every((n) => n.role !== "window"), "the popup's own Pop-up node is not offered");
});

test("a popup's container is offered only when it can scroll more items into view", () => {
  const scrolls = binary([["snapshot --app Notepad --window-id w-a --root @s1:h1", popupRead("w-a", [item("Undo")], ["Scroll", "ScrollTo"])]]);
  assert.deepEqual(readPopups(scope, home, [hint("h1")], scrolls).nodes.map((n) => n.role), ["menu", "menuitem"]);
  const fixed = binary([["snapshot --app Notepad --window-id w-a --root @s1:h1", popupRead("w-a", [item("Undo")])]]);
  assert.deepEqual(readPopups(scope, home, [hint("h1")], fixed).nodes.map((n) => n.role), ["menuitem"]);
});

test("a popup read that answers for another window is discarded", () => {
  const call = binary([["snapshot --app Notepad --window-id w-a --root @s1:h1", popupRead("w-b", [item("Undo")])]]);
  assert.equal(readPopups(scope, home, [hint("h1")], call), null);
  assert.equal(call.calls.length, 1);
});

test("an unpopulated popup is read again a bounded number of times", () => {
  const empty = popupRead("w-a", []);
  const full = popupRead("w-a", [item("Undo"), item("Time/Date")]);
  const call = binary([["snapshot --app Notepad --window-id w-a --root @s1:h1", (n) => (n < 2 ? empty : full)]]);
  assert.deepEqual(names(readPopups(scope, home, [hint("h1")], call).nodes), ["Undo", "Time/Date"]);
  assert.equal(call.calls.length, 2);
  const never = binary([["snapshot --app Notepad --window-id w-a --root @s1:h1", empty]]);
  assert.equal(readPopups(scope, home, [hint("h1")], never), null);
  assert.equal(never.calls.length, 3);
});

test("a binary that cannot read the announced popup reports none, and a hint without a ref is skipped", () => {
  assert.equal(readPopups(scope, home, [hint("h1")], binary([])), null);
  const call = binary([]);
  assert.equal(readPopups(scope, home, [{ role: "window", name: "Pop-up" }], call), null);
  assert.equal(call.calls.length, 0);
});

const bar = () => ({
  ref_id: "@s1:bar", role: "menu", role_description: "menu bar", children_count: 2,
  available_actions: ["ScrollTo"], path: ["window \"Untitled - Notepad\""], parentRole: "group",
});
const barRead = (id, children) => ({ ok: true, data: { window: { id, title: "Untitled - Notepad" }, tree: { ...bar(), children } } });

test("a collapsed menu bar is offered as its entries, taking the control that opens each menu", () => {
  const read = barRead("w-a", [
    { role: "menuitem", name: "File", ref_id: "@s1:file", available_actions: ["ScrollTo"],
      children: [{ role: "button", name: "File", ref_id: "@s1:file-button", available_actions: ["Click", "ScrollTo"] }] },
    { role: "menuitem", name: "Edit", ref_id: "@s1:edit", available_actions: ["Click", "Expand"], states: ["haspopup"],
      children: [{ role: "button", name: "Edit", ref_id: "@s1:edit-button", available_actions: ["Click"] }] },
  ]);
  const call = binary([["snapshot --app Notepad --window-id w-a --root @s1:bar", read]]);
  const nodes = expandMenuBars([bar()], scope, home, call);
  assert.deepEqual(nodes.map((n) => `${n.role}:${n.name}`), ["button:File", "menuitem:Edit"]);
  assert.deepEqual(nodes[0].path, ["window \"Untitled - Notepad\"", "menu"]);
});

test("entries wrapped in an unnamed container are still found", () => {
  const read = barRead("w-a", [{ role: "group", children: [item("File", ["Click", "Expand"]), item("Edit", ["Click", "Expand"])] }]);
  const call = binary([["snapshot --app Notepad --window-id w-a --root @s1:bar", read]]);
  assert.deepEqual(names(expandMenuBars([bar()], scope, home, call)), ["File", "Edit"]);
});

test("the bar stays when its read answers for another window, fails, or holds too much", () => {
  const foreign = binary([["snapshot --app Notepad --window-id w-a --root @s1:bar", barRead("w-b", [item("File")])]]);
  assert.deepEqual(expandMenuBars([bar()], scope, home, foreign).map((n) => n.ref_id), ["@s1:bar"]);
  assert.deepEqual(expandMenuBars([bar()], scope, home, binary([])).map((n) => n.ref_id), ["@s1:bar"]);
  const big = binary([]);
  assert.deepEqual(expandMenuBars([{ ...bar(), children_count: 13 }], scope, home, big).map((n) => n.ref_id), ["@s1:bar"]);
  assert.equal(big.calls.length, 0);
});

test("a trace never carries the text handed to a command, whatever it begins with", () => {
  assert.deepEqual(traced(["set-value", "@s1:e1", "private words"]), ["set-value", "@s1:e1", "[redacted]"]);
  assert.deepEqual(traced(["set-value", "@s1:e1", "@handle"]), ["set-value", "@s1:e1", "[redacted]"]);
  assert.deepEqual(traced(["type", "@s1:e1", "--looks like a flag"]), ["type", "@s1:e1", "[redacted]"]);
  assert.deepEqual(traced(["clipboard-set", "private words"]), ["clipboard-set", "[redacted]"]);
  assert.deepEqual(traced(["clipboard-set", "@restored", "--text"]), ["clipboard-set", "[redacted]", "[redacted]"]);
  assert.deepEqual(traced(["click", "@s1:e1"]), ["click", "@s1:e1"]);
});

test("a menu bar is not an overlay, popup hints are counted, and an expanded entry is found", () => {
  const tree = { role: "window", children: [
    { role: "menu", role_description: "menu bar", children: [item("File", ["Click", "Expand"])] },
    hint("h1"), hint("h2"),
    { role: "group", children: [{ role: "menuitem", name: "View", states: ["expanded", "haspopup"], available_actions: ["Click"] }] },
  ] };
  assert.equal(overlayRole(tree), null);
  assert.equal(popupHints(tree).length, 2);
  assert.equal(hasState(tree, "expanded"), true);
  assert.equal(overlayRole({ role: "window", children: [{ role: "menu", role_description: "menu", children: [] }] }), "menu");
});
