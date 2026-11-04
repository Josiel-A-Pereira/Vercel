/**
 * Which agent-desktop binary the scripts start, and the snapshot flags every
 * read shares. Both entry points read the screen the same way, so both take
 * these from here.
 *
 * On Windows a bare `agent-desktop` cannot be started by execFileSync: npm
 * installs a `.cmd` and a `.ps1` shim, and Node refuses to spawn either without
 * a shell. A shell would re-parse the supplied text, so the native executable
 * the npm package ships is found instead, next to the shim on PATH.
 */
import { existsSync } from "node:fs";
import { delimiter, dirname, join } from "node:path";
import { fileURLToPath } from "node:url";

const REPO = join(dirname(fileURLToPath(import.meta.url)), "..", "..");

const npmNative = (env, arch) => {
  for (const dir of (env.PATH ?? env.Path ?? "").split(delimiter).filter(Boolean)) {
    if (!existsSync(join(dir, "agent-desktop.cmd"))) continue;
    const exe = join(dir, "node_modules", "agent-desktop", "bin", `agent-desktop-win32-${arch}.exe`);
    if (existsSync(exe)) return exe;
  }
  return null;
};

const releaseBuild = (platform) => join(REPO, "target", "release", platform === "win32" ? "agent-desktop.exe" : "agent-desktop");

export const defaultBin = ({ platform = process.platform, arch = process.arch, env = process.env, release = releaseBuild(platform) } = {}) => {
  if (existsSync(release)) return release;
  if (platform === "win32") return npmNative(env, arch) ?? "agent-desktop.exe";
  return "agent-desktop";
};

const bounded = (name, raw, min, max) => {
  if (raw === undefined || raw === "") return null;
  const value = Number(raw);
  if (!Number.isInteger(value) || value < min || value > max) {
    throw new Error(`${name} must be a whole number from ${min} to ${max}, not ${JSON.stringify(raw)}`);
  }
  return String(value);
};

/**
 * A Chromium or Electron window can take longer than the CLI's 3 s default to
 * settle, and its controls often sit deeper than the default depth of 10, where
 * a read returns a tree without them. Both stay at the CLI defaults unless set.
 */
export const snapshotFlags = ({ depth = true, env = process.env } = {}) => {
  const timeout = bounded("JEV_SNAPSHOT_TIMEOUT_MS", env.JEV_SNAPSHOT_TIMEOUT_MS, 1, 120000);
  const maxDepth = depth ? bounded("JEV_SNAPSHOT_MAX_DEPTH", env.JEV_SNAPSHOT_MAX_DEPTH, 1, 100) : null;
  return [...(timeout ? ["--timeout-ms", timeout] : []), ...(maxDepth ? ["--max-depth", maxDepth] : [])];
};

export const pasteCombo = (platform = process.platform) => (platform === "darwin" ? "cmd+v" : "ctrl+v");

/**
 * A minimized window answers a snapshot with an empty tree or a timeout, which
 * reads like a broken application. The window list says which it is.
 */
export const minimizedHint = (cli, app, windowId) => {
  const listed = cli("list-windows", "--app", app);
  if (!listed.ok || !Array.isArray(listed.data)) return null;
  const windows = windowId ? listed.data.filter((w) => w.id === windowId) : listed.data;
  const shut = windows.filter((w) => w.minimized);
  if (!shut.length || shut.length !== windows.length) return null;
  return `the window is minimized; run agent-desktop restore --window-id ${shut[0].id} and try again`;
};
