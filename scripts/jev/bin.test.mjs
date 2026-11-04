import assert from "node:assert/strict";
import { mkdirSync, mkdtempSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";

import { defaultBin, minimizedHint, pasteCombo, snapshotFlags } from "./bin.mjs";

assert.equal(pasteCombo("darwin"), "cmd+v");
assert.equal(pasteCombo("win32"), "ctrl+v");
assert.equal(pasteCombo("linux"), "ctrl+v");

assert.deepEqual(snapshotFlags({ env: {} }), []);
assert.deepEqual(snapshotFlags({ env: { JEV_SNAPSHOT_TIMEOUT_MS: "15000" } }), ["--timeout-ms", "15000"]);
assert.deepEqual(snapshotFlags({ env: { JEV_SNAPSHOT_TIMEOUT_MS: "15000", JEV_SNAPSHOT_MAX_DEPTH: "30" } }),
  ["--timeout-ms", "15000", "--max-depth", "30"]);
assert.deepEqual(snapshotFlags({ depth: false, env: { JEV_SNAPSHOT_MAX_DEPTH: "30" } }), [],
  "a skeleton read keeps the CLI depth so the shallow first look stays shallow");
for (const bad of ["0", "-1", "1.5", "abc", "120001"]) {
  assert.throws(() => snapshotFlags({ env: { JEV_SNAPSHOT_TIMEOUT_MS: bad } }), /JEV_SNAPSHOT_TIMEOUT_MS/, bad);
}
assert.throws(() => snapshotFlags({ env: { JEV_SNAPSHOT_MAX_DEPTH: "101" } }), /JEV_SNAPSHOT_MAX_DEPTH/);

const absent = join(tmpdir(), "jev-no-release-build");
assert.equal(defaultBin({ platform: "linux", env: {}, release: absent }), "agent-desktop");
assert.equal(defaultBin({ platform: "darwin", env: {}, release: absent }), "agent-desktop");

const directory = mkdtempSync(join(tmpdir(), "jev-bin-test-"));
try {
  const shims = join(directory, "npm");
  const native = join(shims, "node_modules", "agent-desktop", "bin");
  mkdirSync(native, { recursive: true });
  writeFileSync(join(shims, "agent-desktop.cmd"), "");
  writeFileSync(join(native, "agent-desktop-win32-x64.exe"), "");
  const empty = join(directory, "elsewhere");
  mkdirSync(empty);
  const path = [empty, shims].join(process.platform === "win32" ? ";" : ":");

  assert.equal(defaultBin({ platform: "win32", arch: "x64", env: { PATH: path }, release: absent }),
    join(native, "agent-desktop-win32-x64.exe"), "the npm shim on PATH leads to the shipped executable");
  assert.equal(defaultBin({ platform: "win32", arch: "x64", env: { Path: path }, release: absent }),
    join(native, "agent-desktop-win32-x64.exe"), "Windows spells the variable Path");
  assert.equal(defaultBin({ platform: "win32", arch: "arm64", env: { PATH: path }, release: absent }), "agent-desktop.exe",
    "an executable for another architecture is not used");
  assert.equal(defaultBin({ platform: "win32", arch: "x64", env: { PATH: empty }, release: absent }), "agent-desktop.exe");
  const release = join(directory, "agent-desktop.exe");
  writeFileSync(release, "");
  assert.equal(defaultBin({ platform: "win32", arch: "x64", env: { PATH: path }, release }), release,
    "a release build in the clone wins over the installed package");
} finally {
  rmSync(directory, { recursive: true, force: true });
}

const listing = (windows) => () => ({ ok: true, data: windows });
assert.match(minimizedHint(listing([{ id: "w-1", minimized: true }]), "App", null), /restore --window-id w-1/);
assert.equal(minimizedHint(listing([{ id: "w-1", minimized: false }]), "App", null), null);
assert.equal(minimizedHint(listing([{ id: "w-1", minimized: true }, { id: "w-2", minimized: false }]), "App", null), null,
  "one open window means minimizing is not why the read failed");
assert.match(minimizedHint(listing([{ id: "w-1", minimized: true }, { id: "w-2", minimized: false }]), "App", "w-1"),
  /w-1/, "a pinned window is judged on its own");
assert.equal(minimizedHint(() => ({ ok: false, error: { code: "APP_NOT_FOUND" } }), "App", null), null);

console.log("ok");
