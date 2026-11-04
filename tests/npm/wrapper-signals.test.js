const assert = require('node:assert/strict');
const { spawn } = require('node:child_process');
const { once } = require('node:events');
const { chmodSync, cpSync, existsSync, mkdirSync, mkdtempSync, readFileSync, rmSync, writeFileSync } = require('node:fs');
const { tmpdir } = require('node:os');
const { delimiter, dirname, join } = require('node:path');
const { test } = require('node:test');
const { resolve } = require('../../npm/lib/platform.js');

for (const signal of ['SIGTERM', 'SIGINT']) {
  test(`terminating the launcher with ${signal} also terminates its native child`, {
    skip: process.platform === 'win32',
    timeout: 5000,
  }, async () => {
    const root = mkdtempSync(join(tmpdir(), 'agent-desktop-launcher-signal-'));
    let launcher;
    let nativePid;
    try {
      const bin = join(root, 'bin');
      mkdirSync(bin);
      cpSync(join(__dirname, '../../npm/bin/agent-desktop'), join(bin, 'agent-desktop'));
      cpSync(join(__dirname, '../../npm/lib'), join(root, 'lib'), { recursive: true });
      writeFileSync(join(root, 'package.json'), JSON.stringify({ version: '9.8.7' }));
      const ready = join(root, 'ready');
      const native = join(bin, resolve(process.platform, process.arch).binaryName);
      writeFileSync(native, [
        '#!/usr/bin/env node',
        'const fs = require("node:fs");',
        'fs.writeFileSync(process.env.READY_PATH + ".tmp", String(process.pid));',
        'fs.renameSync(process.env.READY_PATH + ".tmp", process.env.READY_PATH);',
        'setInterval(() => {}, 1000);',
        '',
      ].join('\n'));
      chmodSync(native, 0o755);
      const helper = join(bin, 'agent-desktop-macos-helper');
      writeFileSync(helper, '', { mode: 0o755 });
      launcher = spawn(process.execPath, [join(bin, 'agent-desktop')], {
        stdio: 'ignore',
        env: {
          ...process.env,
          PATH: `${dirname(process.execPath)}${delimiter}${process.env.PATH || ''}`,
          READY_PATH: ready,
          AGENT_DESKTOP_MACOS_HELPER_PATH: helper,
        },
      });
      for (let attempts = 0; !existsSync(ready) && attempts < 200; attempts++) {
        await new Promise(resolve => setTimeout(resolve, 10));
      }
      assert.ok(existsSync(ready), 'native fixture started before cancellation');
      nativePid = Number(readFileSync(ready, 'utf8'));
      const exited = once(launcher, 'exit');
      launcher.kill(signal);
      await exited;
      assert.throws(() => process.kill(nativePid, 0), { code: 'ESRCH' });
    } finally {
      if (launcher) launcher.kill('SIGKILL');
      if (nativePid) {
        try { process.kill(nativePid, 'SIGKILL'); } catch {}
      }
      rmSync(root, { recursive: true, force: true });
    }
  });
}
