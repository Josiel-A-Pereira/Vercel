const assert = require('node:assert/strict');
const { execFileSync, spawnSync } = require('node:child_process');
const { existsSync, mkdirSync, mkdtempSync, readFileSync, readdirSync, rmSync, writeFileSync } = require('node:fs');
const { tmpdir } = require('node:os');
const { dirname, join, resolve } = require('node:path');
const { test } = require('node:test');

const version = '0.10.2';
const tag = `v${version}`;
const cargo = '[package]\nversion = "9.9.9"\n\n[workspace.package]\nversion = "0.10.2"\n\n[dependencies]\nversion = "8.8.8"\n';
const authoredNotes = [
  'Keep the authored [guide](https://example.invalid/guide) and punctuation.',
  '',
  '### Bug Fixes',
  '',
  '- Pin release notes to the source commit.  ',
  '  Keep this continuation with `code` and two trailing spaces.  ',
  '- Literal examples: $(touch publish-substitution) and `touch publish-backticks`; $RELEASE_VERSION.',
  '',
  '### Contributors',
  '',
  '- @release-fixture',
  '- @Alice',
  '',
].join('\n');
const changelog = '# Changelog\n\n## [Unreleased]\n\nFuture work must not leak.\n\n'
  + '## [0.10.2](https://example.invalid/compare/v0.10.1...v0.10.2) (2026-11-05)\n\n'
  + authoredNotes + '\n## 0.10.1\n\nOlder notes must not leak.\n';
const archives = [
  './agent-desktop-darwin-arm64.tar.gz',
  './agent-desktop-linux-x64.tar.gz',
  './agent-desktop-windows-arm64.zip',
  './agent-desktop-windows-x64.zip',
  'checksums.txt',
];
const viewArgs = ['release', 'view', tag];
const uploadArgs = ['release', 'upload', tag, ...archives, '--clobber'];
const ghStub = `#!/usr/bin/env node
const { appendFileSync, readFileSync } = require('node:fs');
const args = process.argv.slice(2);
const call = { args };
if (args[0] === 'release' && args[1] === 'create') {
  const index = args.indexOf('--notes-file');
  if (index !== -1) {
    call.notesFile = args[index + 1];
    call.notes = readFileSync(call.notesFile, 'utf8');
  }
}
appendFileSync(process.env.TEST_GH_LOG, JSON.stringify(call) + '\\n');
if (args[0] !== 'release') process.exit(99);
if (args[1] === 'view') process.exit(process.env.TEST_RELEASE_EXISTS === '1' ? 0 : 1);
if (args[1] === 'create') process.exit(Number(process.env.TEST_CREATE_STATUS));
if (args[1] === 'upload') process.exit(0);
process.exit(99);
`;

function fixture(t, { manifest = cargo, markdown = changelog } = {}) {
  const root = mkdtempSync(join(tmpdir(), 'ad-publish-'));
  t.after(() => rmSync(root, { recursive: true, force: true }));
  const env = {
    PATH: process.env.PATH, HOME: root, LANG: 'C',
    GIT_CONFIG_NOSYSTEM: '1', GIT_CONFIG_GLOBAL: '/dev/null',
  };
  const git = (...args) => execFileSync('git', [
    '-c', 'commit.gpgsign=false', '-c', 'core.hooksPath=/dev/null', ...args,
  ], { cwd: root, env, encoding: 'utf8' }).trim();
  const commit = () => {
    git('add', '-A');
    git('commit', '-qm', 'test: release fixture');
    return git('rev-parse', 'HEAD');
  };
  writeFileSync(join(root, 'Cargo.toml'), manifest);
  if (markdown !== null) writeFileSync(join(root, 'CHANGELOG.md'), markdown);
  git('init', '-q', '--template=', '--initial-branch=laf/release-fixture');
  git('config', 'user.name', 'Release Fixture');
  git('config', 'user.email', 'fixture@example.invalid');
  const source = commit();
  const tools = join(root, 'tools');
  const notesDir = join(root, 'runner temp');
  mkdirSync(tools);
  mkdirSync(notesDir);
  writeFileSync(join(tools, 'gh'), ghStub, { mode: 0o755 });
  for (const path of archives) writeFileSync(join(root, path), `Fixture asset: ${path}\n`);
  writeFileSync(join(root, 'not-an-asset.txt'), 'Do not upload this file.\n');
  const log = join(root, 'gh.jsonl');
  const publish = (overrides = {}) => {
    writeFileSync(log, '');
    const result = spawnSync('bash', [resolve(__dirname, '../../scripts/publish-github-release.sh')], {
      cwd: root, encoding: 'utf8',
      env: {
        ...env, PATH: `${tools}:${env.PATH}`, RUNNER_TEMP: notesDir,
        RELEASE_VERSION: version, RELEASE_SOURCE_SHA: source,
        RELEASE_WORKFLOW_SHA: git('rev-parse', 'HEAD'),
        TEST_GH_LOG: log, TEST_RELEASE_EXISTS: '0', TEST_CREATE_STATUS: '0',
        ...overrides,
      },
    });
    assert.ifError(result.error);
    const calls = readFileSync(log, 'utf8').split('\n').filter(Boolean).map((line) => JSON.parse(line));
    assert.deepEqual(readdirSync(notesDir), [], 'Temporary release notes must be removed on every exit');
    return { result, calls };
  };
  return { root, git, commit, source, notesDir, publish };
}

function assertRejected({ result, calls }, error) {
  assert.equal(result.status, 1, result.stderr);
  assert.equal(result.stdout, '');
  assert.match(result.stderr, error);
  assert.deepEqual(calls, [], 'Invalid release input must fail before calling gh');
}

function assertCreate(call, notesDir) {
  assert.deepEqual(call.args, [
    'release', 'create', tag, '--verify-tag', '--title', tag, '--notes-file', call.notesFile,
  ]);
  assert.equal(dirname(call.notesFile), notesDir);
  assert.equal(call.notes, authoredNotes);
  assert.equal(existsSync(call.notesFile), false);
}

test('creates a release with exact pinned notes and uploads all archives and checksums', { skip: process.platform === 'win32' }, (t) => {
  const f = fixture(t);
  writeFileSync(join(f.root, 'Cargo.toml'), cargo.replace('0.10.2', '0.10.3'));
  writeFileSync(join(f.root, 'CHANGELOG.md'), changelog.replaceAll('0.10.2', '0.10.3')
    .replace('Pin release notes to the source commit.', 'Later checkout notes must not be published.'));
  const workflow = f.commit();
  const { result, calls } = f.publish({ RELEASE_WORKFLOW_SHA: workflow });
  assert.equal(result.status, 0, result.stderr);
  assert.equal(result.stdout, '');
  assert.equal(result.stderr, '');
  assert.equal(calls.length, 3);
  assert.deepEqual(calls[0].args, viewArgs);
  assertCreate(calls[1], f.notesDir);
  assert.deepEqual(calls[2].args, uploadArgs);
  assert.equal(f.git('rev-parse', 'HEAD'), workflow);
  assert.equal(existsSync(join(f.root, 'publish-substitution')), false);
  assert.equal(existsSync(join(f.root, 'publish-backticks')), false);
});

test('an existing release only uploads assets without creating or editing its body', { skip: process.platform === 'win32' }, (t) => {
  const f = fixture(t);
  const { result, calls } = f.publish({ TEST_RELEASE_EXISTS: '1' });
  assert.equal(result.status, 0, result.stderr);
  assert.equal(result.stderr, '');
  assert.deepEqual(calls, [{ args: viewArgs }, { args: uploadArgs }]);
});

test('rejects malformed source and workflow SHAs before gh', { skip: process.platform === 'win32' }, async (t) => {
  const f = fixture(t);
  for (const scenario of [
    { name: 'missing source', env: { RELEASE_SOURCE_SHA: '' } },
    { name: 'abbreviated source', env: { RELEASE_SOURCE_SHA: f.source.slice(0, 12) } },
    { name: 'shell-shaped source', env: { RELEASE_SOURCE_SHA: 'HEAD; exit 0' } },
    { name: 'missing workflow', env: { RELEASE_WORKFLOW_SHA: '' } },
    { name: 'abbreviated workflow', env: { RELEASE_WORKFLOW_SHA: f.source.slice(0, 12) } },
  ]) {
    await t.test(scenario.name, () => assertRejected(f.publish(scenario.env), /full commit SHAs/i));
  }
});

test('requires the source to be an ancestor of the workflow commit', { skip: process.platform === 'win32' }, async (t) => {
  const f = fixture(t);
  writeFileSync(join(f.root, 'later.txt'), 'Later trusted history.\n');
  const workflow = f.commit();
  const unrelated = f.git('commit-tree', `${f.source}^{tree}`, '-m', 'test: unrelated root');
  for (const scenario of [
    { name: 'unrelated history', source: unrelated, workflow },
    { name: 'nonexistent commit', source: 'f'.repeat(40), workflow },
    { name: 'source newer than workflow', source: workflow, workflow: f.source },
  ]) {
    await t.test(scenario.name, () => {
      assertRejected(f.publish({
        RELEASE_SOURCE_SHA: scenario.source, RELEASE_WORKFLOW_SHA: scenario.workflow,
      }), /triggering main history/i);
    });
  }
});

test('validates the pinned workspace version even when the current manifest is valid', { skip: process.platform === 'win32' }, async (t) => {
  for (const scenario of [
    { name: 'version mismatch', manifest: '[workspace.package]\nversion = "0.10.1"\n', error: /version does not match/i },
    { name: 'package version without workspace version', manifest: '[package]\nversion = "0.10.2"\n', error: /valid workspace\.package\.version/i },
    { name: 'invalid workspace version', manifest: '[workspace.package]\nversion = "not-semver"\n', error: /valid workspace\.package\.version/i },
  ]) {
    await t.test(scenario.name, (t) => {
      const f = fixture(t, { manifest: scenario.manifest });
      writeFileSync(join(f.root, 'Cargo.toml'), cargo);
      f.commit();
      assertRejected(f.publish(), scenario.error);
    });
  }
});

test('validates pinned notes before gh despite a corrected current changelog', { skip: process.platform === 'win32' }, async (t) => {
  for (const scenario of [
    { name: 'missing changelog at source', markdown: null, error: /CHANGELOG\.md/i },
    { name: 'missing release entry', markdown: changelog.replaceAll('0.10.2', '0.10.3'), error: /0\.10\.2.*not found/i },
    { name: 'missing contributor credits', markdown: changelog.replace(/### Contributors\n[\s\S]*?(?=\n## 0\.10\.1)/, ''), error: /Contributors/i },
  ]) {
    await t.test(scenario.name, (t) => {
      const f = fixture(t, { markdown: scenario.markdown });
      writeFileSync(join(f.root, 'CHANGELOG.md'), changelog);
      f.commit();
      assertRejected(f.publish(), scenario.error);
    });
  }
});

test('failed release creation stops before upload and removes temporary notes', { skip: process.platform === 'win32' }, (t) => {
  const f = fixture(t);
  const { result, calls } = f.publish({ TEST_CREATE_STATUS: '17' });
  assert.equal(result.status, 17, result.stderr);
  assert.equal(result.stdout, '');
  assert.equal(calls.length, 2);
  assert.deepEqual(calls[0].args, viewArgs);
  assertCreate(calls[1], f.notesDir);
});
