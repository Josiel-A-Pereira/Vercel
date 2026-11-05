const assert = require('node:assert/strict');
const { mkdirSync, mkdtempSync, readFileSync, writeFileSync } = require('node:fs');
const { tmpdir } = require('node:os');
const { join, relative } = require('node:path');
const { test } = require('node:test');
const optimize = require('../../npm/lib/windows-bin');

test('Windows optimization changes only command shims targeting this installation', () => {
  const root = mkdtempSync(join(tmpdir(), 'ad-windows-bin-'));
  const bin = join(root, 'node_modules/agent-desktop/bin');
  mkdirSync(bin, { recursive: true });
  const binary = join(bin, 'agent-desktop-win32-x64.exe');
  writeFileSync(binary, 'native');
  const command = join(root, 'agent-desktop.cmd');
  const otherCommand = join(root, 'node_modules/.bin/agent-desktop.cmd');
  mkdirSync(join(root, 'node_modules/.bin'), { recursive: true });
  const unrelated = '@echo off\nnode other-package/cli.js %*\n';
  writeFileSync(otherCommand, unrelated);
  const target = relative(root, join(bin, 'agent-desktop'));
  writeFileSync(command, `@echo off\nnode "${target}" %*\n`);
  assert.equal(optimize(bin, 'agent-desktop-win32-x64.exe'), true);
  const installed = readFileSync(command, 'utf8');
  const local = relative(root, binary).replace(/\//g, '\\');
  assert.ok(installed.includes(`"%~dp0${local}" %*`));
  assert.ok(!installed.includes(root));
  assert.doesNotMatch(installed, /node(?:\.exe)?[ "\t]/);
  assert.equal(readFileSync(otherCommand, 'utf8'), unrelated);
});

test('Windows shims locate the binary relative to themselves so a moved install keeps working', () => {
  const root = mkdtempSync(join(tmpdir(), 'ad-windows-bin-'));
  const bin = join(root, 'node_modules/agent-desktop/bin');
  mkdirSync(bin, { recursive: true });
  writeFileSync(join(bin, 'agent-desktop-win32-x64.exe'), 'native');
  const target = relative(root, join(bin, 'agent-desktop'));
  writeFileSync(join(root, 'agent-desktop.cmd'), `@echo off\nnode "${target}" %*\n`);
  writeFileSync(join(root, 'agent-desktop.ps1'), `& node "$basedir/${target}" $args\n`);
  assert.equal(optimize(bin, 'agent-desktop-win32-x64.exe'), true);
  const local = 'node_modules\\agent-desktop\\bin\\agent-desktop-win32-x64.exe';
  assert.equal(
    readFileSync(join(root, 'agent-desktop.cmd'), 'utf8'),
    `@echo off\r\n"%~dp0${local}" %*\r\nexit /b %errorlevel%\r\n`,
  );
  assert.equal(
    readFileSync(join(root, 'agent-desktop.ps1'), 'utf8'),
    `& (Join-Path $PSScriptRoot '${local}') @args\r\nexit $LASTEXITCODE\r\n`,
  );
});
