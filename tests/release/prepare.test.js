const assert = require('node:assert/strict');
const { execFileSync, spawnSync } = require('node:child_process');
const { mkdirSync, mkdtempSync, readFileSync, rmSync, writeFileSync } = require('node:fs');
const { tmpdir } = require('node:os');
const { join, resolve } = require('node:path');
const { test } = require('node:test');
const { releaseVersion } = require('../../scripts/release-version');

const changelog = `# Changelog

## [0.10.2](https://github.com/fixture/agent-desktop/compare/v0.10.1...v0.10.2) (2026-11-05)

### Bug Fixes

* Preserve release source provenance. ([#1](https://github.com/fixture/agent-desktop/pull/1)) by @release-fixture

### Contributors

- @release-fixture
`;
const notesWithoutCredit = changelog.slice(0, changelog.indexOf('### Contributors'));

function fixture() {
  const root = mkdtempSync(join(tmpdir(), 'ad-release-gate-'));
  for (const name of ['scripts', 'npm', 'tools']) mkdirSync(join(root, name));
  writeFileSync(join(root, 'npm/package.json'), '{"version":"0.9.0"}');
  writeFileSync(join(root, 'Cargo.toml'), '[workspace.package]\nversion = "0.10.2"\n');
  writeFileSync(join(root, 'CHANGELOG.md'), changelog);
  writeFileSync(join(root, 'scripts/check-release-consistency.sh'), 'exit 0\n');
  writeFileSync(join(root, 'tools/npm'), '#!/bin/sh\nprintf "%s\\n" "$TEST_NPM_VERSION"\n', { mode: 0o755 });
  writeFileSync(join(root, 'tools/gh'), '#!/bin/sh\nif [ "$1" = release ]; then exit "$TEST_GITHUB_STATUS"; fi\nprintf "%s\\n" "$*" >> "$TEST_GH_LOG"\n', { mode: 0o755 });
  const git = (...args) => execFileSync('git', args, { cwd: root, encoding: 'utf8' }).trim();
  git('init', '-q');
  git('config', 'user.name', 'Release Fixture');
  git('config', 'user.email', 'fixture@example.invalid');
  git('add', '.');
  git('commit', '-qm', 'test: create release fixture');
  return { root, git };
}

function prepare(root, npmVersion = '0.10.1', githubStatus = '1') {
  const output = join(root, 'output');
  const log = join(root, 'gh.log');
  writeFileSync(output, '');
  writeFileSync(log, '');
  const result = spawnSync('bash', [resolve(__dirname, '../../scripts/prepare-release.sh')], {
    cwd: root, encoding: 'utf8',
    env: { ...process.env, PATH: `${join(root, 'tools')}:${process.env.PATH}`,
      GH_REPO: 'fixture/agent-desktop', GITHUB_OUTPUT: output,
      TEST_NPM_VERSION: npmVersion, TEST_GITHUB_STATUS: githubStatus, TEST_GH_LOG: log },
  });
  const values = Object.fromEntries(readFileSync(output, 'utf8').trim().split('\n').filter(Boolean).map((line) => line.split('=')));
  return { result, values, calls: readFileSync(log, 'utf8') };
}

for (const scenario of [
  { name: 'unchanged published version skips publication with a corrupt changelog', npm: '0.10.2', github: '0', release: 'false', githubRelease: 'false', corrupt: true },
  { name: 'unchanged published version skips publication without a changelog', npm: '0.10.2', github: '0', release: 'false', githubRelease: 'false', missing: true },
  { name: 'new version prepares publication', npm: '0.10.1', github: '1', release: 'true', githubRelease: 'true' },
  { name: 'missing GitHub release rebuilds the tagged source without republishing npm', npm: '0.10.2', github: '1', release: 'false', githubRelease: 'true', tagged: true, corrupt: true },
  { name: 'npm retry uses the existing tag despite corrupt checkout notes', npm: '0.10.1', github: '1', release: 'true', githubRelease: 'true', tagged: true, corrupt: true },
]) {
  test(scenario.name, { skip: process.platform === 'win32' }, () => {
    const { root, git } = fixture();
    const source = git('rev-parse', 'HEAD');
    if (scenario.tagged) git('tag', '--no-sign', '-m', 'test: release fixture', 'v0.10.2');
    if (scenario.corrupt) writeFileSync(join(root, 'CHANGELOG.md'), '# Invalid release notes\n');
    if (scenario.missing) rmSync(join(root, 'CHANGELOG.md'));
    if (scenario.tagged) {
      writeFileSync(join(root, 'later.txt'), 'subsequent main commit');
      git('add', '.');
      git('commit', '-qm', 'test: advance main after tag');
    }
    const { result, values, calls } = prepare(root, scenario.npm, scenario.github);
    assert.equal(result.status, 0, result.stderr);
    assert.equal(result.stdout, '');
    assert.equal(values.should_release, scenario.release);
    assert.equal(values.needs_github_release, scenario.githubRelease);
    assert.equal(values.source_sha, source);
    assert.equal(values.tag_name, 'v0.10.2');
    assert.equal(values.version, '0.10.2');
    if (scenario.release === 'true' && !scenario.tagged) {
      assert.equal(calls, `api repos/fixture/agent-desktop/git/refs -f ref=refs/tags/v0.10.2 -f sha=${source}\n`);
    } else {
      assert.equal(calls, '');
    }
  });
}

for (const scenario of [
  { name: 'missing changelog', notes: null, error: /CHANGELOG\.md/i },
  { name: 'different release version', notes: changelog.replaceAll('0.10.2', '0.10.1'), error: /0\.10\.2/ },
  { name: 'missing contributor credits', notes: notesWithoutCredit, error: /contributor|credit/i },
  { name: 'missing change bullets', notes: changelog.replace(/^\* Preserve.*\n/m, ''), error: /change|bullet|notes|entry/i },
  { name: 'unclosed comment swallowing an older release', notes: `${changelog}\n<!--\n## 0.10.1\nOlder notes.\n`, error: /unclosed HTML comment/i },
  { name: 'contributor credit inside indented code', notes: changelog.replace('\n- @release-fixture', '\n    - @release-fixture'), error: /contributor/i },
]) {
  test(`new release rejects ${scenario.name} before creating a tag`, { skip: process.platform === 'win32' }, () => {
    const { root } = fixture();
    if (scenario.notes === null) rmSync(join(root, 'CHANGELOG.md'));
    else writeFileSync(join(root, 'CHANGELOG.md'), scenario.notes);
    const { result, values, calls } = prepare(root);
    assert.equal(result.status, 1, result.stderr);
    assert.match(result.stderr, scenario.error);
    assert.equal(result.stdout, '');
    assert.deepEqual(values, {});
    assert.equal(calls, '');
  });
}

for (const scenario of [
  { name: 'invalid release notes', file: 'CHANGELOG.md', content: notesWithoutCredit, error: /contributor|credit/i },
  { name: 'inconsistent Cargo version', file: 'Cargo.toml', content: '[workspace.package]\nversion = "0.10.1"\n', error: /inconsistent version metadata/i },
]) {
  test(`existing tag with ${scenario.name} fails despite a fixed checkout`, { skip: process.platform === 'win32' }, () => {
    const { root, git } = fixture();
    const original = readFileSync(join(root, scenario.file), 'utf8');
    writeFileSync(join(root, scenario.file), scenario.content);
    git('add', '.');
    git('commit', '-qm', 'test: create invalid release source');
    git('tag', '--no-sign', '-m', 'test: invalid release fixture', 'v0.10.2');
    writeFileSync(join(root, scenario.file), original);
    git('add', '.');
    git('commit', '-qm', 'test: fix current checkout after tag');
    const { result, values, calls } = prepare(root, '0.10.2');
    assert.equal(result.status, 1, result.stderr);
    assert.match(result.stderr, scenario.error);
    assert.equal(result.stdout, '');
    assert.deepEqual(values, {});
    assert.equal(calls, '');
  });
}

test('release checkout accepts only commits in the triggering history', { skip: process.platform === 'win32' }, () => {
  const { root, git } = fixture();
  const ancestor = git('rev-parse', 'HEAD');
  writeFileSync(join(root, 'later.txt'), 'trusted main change');
  git('add', '.');
  git('commit', '-qm', 'test: advance trusted main');
  const workflow = git('rev-parse', 'HEAD');
  git('checkout', '--orphan', 'unrelated');
  git('commit', '-qm', 'test: create unrelated history');
  const unrelated = git('rev-parse', 'HEAD');
  git('checkout', '--detach', workflow);
  for (const scenario of [
    { source: unrelated }, { source: 'main; exit 0' }, { source: ancestor },
    { source: ancestor, differentOwner: '1' },
  ]) {
    git('checkout', '--detach', workflow);
    const { source, differentOwner } = scenario;
    const result = spawnSync('bash', [resolve(__dirname, '../../scripts/checkout-release-source.sh')], {
      cwd: root, encoding: 'utf8',
      env: { ...process.env, RELEASE_SOURCE_SHA: source, RELEASE_WORKFLOW_SHA: workflow,
        GIT_TEST_ASSUME_DIFFERENT_OWNER: differentOwner },
    });
    assert.equal(result.status, source === ancestor ? 0 : 1, result.stderr);
    assert.equal(git('rev-parse', 'HEAD'), source === ancestor ? ancestor : workflow);
  }
});

test('version comes only from the workspace package section', () => {
  const toml = '[package]\nversion = "9.9.9"\n[workspace.package]\nversion = "1.2.3"\n[dependencies]\nversion = "8.8.8"\n';
  assert.equal(releaseVersion(toml), '1.2.3');
  assert.equal(releaseVersion(toml.replaceAll('\n', '\r\n')), '1.2.3');
  for (const version of ['', '1.2', 'v1.2.3', '1.2.3;echo bad', '01.2.3']) {
    assert.throws(() => releaseVersion(`[workspace.package]\nversion = "${version}"\n`));
  }
  assert.throws(() => releaseVersion('[package]\nversion = "1.2.3"\n'));
});
