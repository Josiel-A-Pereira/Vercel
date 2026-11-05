const assert = require('node:assert/strict');
const { spawnSync } = require('node:child_process');
const { mkdtempSync, rmSync, writeFileSync } = require('node:fs');
const { tmpdir } = require('node:os');
const { join, resolve } = require('node:path');
const { test } = require('node:test');
const { releaseNotes } = require('../../scripts/release-notes');

const body = [
  'A short introduction with [a guide](https://example.invalid/guide).',
  '',
  '### New Features',
  '',
  '- Keep authored wording, without requiring bold text or a PR link.',
  '  A continuation with `code` and two trailing spaces.  ',
  '',
  '### Contributors',
  '',
  '- @Alice',
  '- @dev-two',
  '',
].join('\n');

function entry(content = body, version = '1.2.3') {
  return `## ${version}\n\n${content}`;
}

function cliFixture(t, markdown = entry()) {
  const root = mkdtempSync(join(tmpdir(), 'ad-release-notes-'));
  t.after(() => rmSync(root, { recursive: true, force: true }));
  writeFileSync(join(root, 'CHANGELOG.md'), markdown);
  writeFileSync(join(root, 'release notes.md'), markdown);
  return (args, input) => spawnSync(process.execPath, [resolve(__dirname, '../../scripts/release-notes.js'), ...args], {
    cwd: root, encoding: 'utf8', input,
  });
}

test('returns only the latest release body without rewriting Markdown', () => {
  const markdown = '# Changelog\n\n## [Unreleased]\n\nFuture content.\n\n'
    + '## [1.2.3](https://github.com/vercel-labs/agent-desktop/compare/v1.2.2...v1.2.3) (2026-11-05)\n\n'
    + body + '\n\n' + entry('Older release content must not leak.\n', '1.2.2');
  assert.equal(releaseNotes(markdown, '1.2.3'), body);
});

test('accepts plain headings, an entry at EOF, and preserves CRLF', () => {
  assert.equal(releaseNotes(entry().trimEnd(), '1.2.3'), body);
  const crlf = entry().replaceAll('\n', '\r\n');
  assert.equal(releaseNotes(crlf, '1.2.3'), body.replaceAll('\n', '\r\n'));
});

test('supports semver prereleases and build metadata', () => {
  for (const version of ['0.0.0', '1.2.3-rc.1', '1.2.3-alpha-beta.0', '1.2.3+build.01', '1.2.3-rc.1+build.2']) {
    assert.equal(releaseNotes(entry(body, version), version), body);
  }
});

test('rejects invalid requested versions, missing versions, and older selections', () => {
  for (const version of [undefined, '', '1.2', 'v1.2.3', '01.2.3', '1.02.3', '1.2.03', '1.2.3-01', '1.2.3-rc..1', '1.2.3+', '1.2.3\n', ' 1.2.3', '1.2.3;echo bad']) {
    assert.throws(() => releaseNotes(entry(), version), /valid release version/);
  }
  assert.throws(() => releaseNotes(entry(), '9.9.9'), /not found/);
  assert.throws(() => releaseNotes(entry(body, '1.2.4') + '\n' + entry(), '1.2.3'), /latest numbered release/);
  assert.throws(() => releaseNotes(entry() + '\n' + entry(body, '1.2.2'), '1.2.2'), /latest numbered release/);
});

test('rejects duplicate or malformed release headings rather than guessing', () => {
  const linked = '## [1.2.3](https://example.invalid/release) (2026-11-05)\n' + body;
  assert.throws(() => releaseNotes(entry() + '\n' + linked, '1.2.3'), /Duplicate release heading/);
  for (const heading of ['1.2', 'v1.2.3', '1.2.3 trailing text', '[1.2.3](url', '[1.2.3-01](url)']) {
    assert.throws(() => releaseNotes(`## ${heading}\n${body}\n${entry()}`, '1.2.3'), /Malformed release heading/);
  }
});

test('accepts each curated change group and legacy breaking-change headings', () => {
  for (const title of ['New Features', 'Features', 'Bug Fixes', 'Improvements', 'Documentation', 'Breaking Changes', '⚠ BREAKING CHANGES', '⚠️ BREAKING CHANGES']) {
    const content = `### ${title}\n\nA meaningful change can be a paragraph.\n\n### Contributors\n\n- @a\n`;
    assert.equal(releaseNotes(entry(content), '1.2.3'), content);
  }
});

test('rejects empty, placeholder-only, contributor-only, and empty change groups', () => {
  for (const changes of ['', '   ', '- ', '- TODO', '* **TBD**', '- None.', '- N/A', '- ...', '<!-- add release notes here -->', '#### Details']) {
    const content = `### New Features\n\n${changes}\n\n### Contributors\n\n- @Alice\n`;
    assert.throws(() => releaseNotes(entry(content), '1.2.3'), /empty or contains only placeholders/);
  }
  assert.throws(() => releaseNotes(entry(''), '1.2.3'), /nonempty change group/);
  assert.throws(() => releaseNotes(entry('### Contributors\n\n- @Alice\n'), '1.2.3'), /nonempty change group/);
  const content = body.replace('### Contributors', '### Bug Fixes\n\n<!-- Nothing written yet. -->\n\n### Contributors');
  assert.throws(() => releaseNotes(entry(content), '1.2.3'), /Bug Fixes is empty/);
});

test('requires exactly one populated Contributors section', () => {
  assert.throws(() => releaseNotes(entry(body.split('### Contributors')[0]), '1.2.3'), /exactly one Contributors/);
  assert.throws(() => releaseNotes(entry(body + '\n### Contributors\n\n- @Bob\n'), '1.2.3'), /exactly one Contributors/);
  for (const credit of ['', '   ', '<!-- add handles -->']) {
    assert.throws(() => releaseNotes(entry(`### Bug Fixes\n\n- Fix a bug.\n\n### Contributors\n\n${credit}\n`), '1.2.3'), /at least one - @handle/);
  }
});

test('validates GitHub handle syntax and refuses bots and duplicate credits', () => {
  for (const credit of ['- @-alice', '- @alice-', '- @a--b', '- @a_b', '- @a.b', '- @éclair', `- @${'a'.repeat(40)}`, '- @dependabot[bot]', '- @github-actions[bot]', '- @', '- Alice', '- @Alice extra']) {
    const content = body.replace('- @Alice\n- @dev-two', credit);
    assert.throws(() => releaseNotes(entry(content), '1.2.3'), /valid non-bot GitHub handles/);
  }
  assert.throws(() => releaseNotes(entry(body.replace('- @dev-two', '- @aLiCe')), '1.2.3'), /Duplicate contributor/);
  const content = body.replace('- @Alice\n- @dev-two', `- @1\n- @${'a'.repeat(39)}\n- @a-b-c`);
  assert.equal(releaseNotes(entry(content), '1.2.3'), content);
});

test('fenced apparent release and section headings stay in the authored body', () => {
  for (const fence of ['```', '~~~~', '   ````']) {
    const marker = fence.trim();
    const example = [
      `${fence}markdown`,
      '## 9.9.9',
      '## 1.2.3',
      '### Contributors',
      '- @fake[bot]',
      marker[0] === '`' ? '~~~' : '```',
      marker.slice(1),
      marker,
    ].join('\n');
    const content = body.replace('### Contributors', `${example}\n\n### Contributors`);
    assert.equal(releaseNotes(entry(content) + '\n' + entry('Older notes.\n', '1.2.2'), '1.2.3'), content);
  }
});

test('code samples cannot supply required headings or contributor entries', () => {
  const fakeChanges = '```markdown\n### New Features\n\n- This is just an example.\n```\n\n### Contributors\n\n- @Alice\n';
  assert.throws(() => releaseNotes(entry(fakeChanges), '1.2.3'), /nonempty change group/);
  const fakeContributors = '### Bug Fixes\n\n- Fix a bug.\n\n```markdown\n### Contributors\n\n- @Alice\n```\n';
  assert.throws(() => releaseNotes(entry(fakeContributors), '1.2.3'), /exactly one Contributors/);
  const fakeHandles = body.replace('- @Alice\n- @dev-two', '```\n- @Alice\n```');
  assert.throws(() => releaseNotes(entry(fakeHandles), '1.2.3'), /at least one - @handle/);
  const fakeContent = '### New Features\n\n```\n- This is just an example.\n```\n\n### Contributors\n\n- @Alice\n';
  assert.throws(() => releaseNotes(entry(fakeContent), '1.2.3'), /empty or contains only placeholders/);
});

test('ignores commented headings and fences without changing the returned body', () => {
  const example = '<!--\n## 1.2.3\n### Contributors\n```\n-->\n\n```html\n<!-- an example, not a changelog comment\n```\n';
  const content = body.replace('### Contributors', `${example}\n### Contributors`);
  assert.equal(releaseNotes(entry(content), '1.2.3'), content);
  const commentedGroup = '<!--\n### New Features\n\n- An example.\n-->\n\n### Contributors\n\n- @Alice\n';
  assert.throws(() => releaseNotes(entry(commentedGroup), '1.2.3'), /nonempty change group/);
});

test('rejects unclosed fences even if they would hide an older entry', () => {
  for (const fence of ['```', '~~~', '````\n```', '```\n~~~']) {
    assert.throws(() => releaseNotes(entry(body + '\n' + fence + '\n## 1.2.2\n'), '1.2.3'), /unclosed code fence/);
  }
});

test('literal comment tokens in inline and indented code do not invalidate current or historical notes', () => {
  for (const example of [
    '- Render literal `<!--` text.',
    '- Render ``a `<!--` token`` as code.',
    '- Render `a multiline\n<!-- token` as code.',
    '    <!-- an indented code example',
    '\t<!-- a tab-indented code example',
  ]) {
    const content = body.replace('### Contributors', `${example}\n\n### Contributors`);
    assert.equal(releaseNotes(entry(content), '1.2.3'), content);
    assert.equal(releaseNotes(entry() + '\n' + entry(`${example}\n`, '1.2.2'), '1.2.3'), body);
  }
});

test('unclosed comments cannot hide boundaries and pull older notes into the release', () => {
  const markdown = entry(body + '\n<!--\n') + '\n' + entry('Older notes.\n', '1.2.2');
  assert.throws(() => releaseNotes(markdown, '1.2.3'), /unclosed HTML comment/);
});

test('indented code cannot supply change text or contributor credits', () => {
  for (const indent of ['    ', '\t', '  \t']) {
    const example = `${indent}- @example`;
    const fakeCredit = body.replace('- @Alice\n- @dev-two', example);
    assert.throws(() => releaseNotes(entry(fakeCredit), '1.2.3'), /at least one - @handle/);
    const fakeChange = `### Bug Fixes\n\n${example}\n\n### Contributors\n\n- @Alice\n`;
    assert.throws(() => releaseNotes(entry(fakeChange), '1.2.3'), /empty or contains only placeholders/);
  }
});

test('CLI default file, explicit file, and stdin return exact notes with exit zero', (t) => {
  const run = cliFixture(t, entry() + '\n' + entry('Older content.\n', '1.2.2'));
  for (const args of [['1.2.3'], ['1.2.3', 'release notes.md'], ['1.2.3', '-']]) {
    const result = run(args, entry());
    assert.equal(result.status, 0, result.stderr);
    assert.equal(result.stderr, '');
    assert.equal(result.stdout, body);
  }
});

test('CLI errors exit one on stderr without partial stdout', (t) => {
  const run = cliFixture(t);
  for (const scenario of [
    { args: [], error: /Usage:/ },
    { args: ['bad-version'], error: /valid release version/ },
    { args: ['9.9.9'], error: /not found/ },
    { args: ['1.2.3', 'missing.md'], error: /ENOENT/ },
    { args: ['1.2.3', '-', 'extra'], error: /Usage:/ },
    { args: ['1.2.3', '-'], input: entry(body.replace('- @dev-two', '- @alice')), error: /Duplicate contributor/ },
    { args: ['1.2.3', '-'], input: entry() + '\n' + entry(), error: /Duplicate release heading/ },
    { args: ['1.2.3', '-'], input: entry(body, '1.2.4') + '\n' + entry(), error: /latest numbered release/ },
  ]) {
    const result = run(scenario.args, scenario.input);
    assert.equal(result.status, 1, result.stderr);
    assert.equal(result.stdout, '');
    assert.match(result.stderr, scenario.error);
    assert.doesNotMatch(result.stderr, /\n\s+at /);
  }
});
