const assert = require('node:assert/strict');
const { readFileSync } = require('node:fs');
const { resolve } = require('node:path');
const { test } = require('node:test');

const workflow = readFileSync(resolve(__dirname, '../../.github/workflows/release.yml'), 'utf8').replaceAll('\r\n', '\n');
const publisher = workflow.split('\n  publish-github:\n')[1]?.split(/^  [a-z][a-z-]*:\s*$/m)[0];

test('the GitHub publisher binds curated notes to the checked release source', () => {
  assert.ok(publisher, 'release workflow must have a GitHub publisher');
  assert.match(publisher, /uses: actions\/checkout@[^\n]+\n\s+with:\n\s+persist-credentials: false\n\s+fetch-depth: 0\n\s+ref: \$\{\{ github\.sha \}\}/);
  assert.match(publisher, /RELEASE_VERSION: \$\{\{ needs\.check-release\.outputs\.version \}\}/);
  assert.match(publisher, /RELEASE_SOURCE_SHA: \$\{\{ needs\.check-release\.outputs\.source_sha \}\}/);
  assert.match(publisher, /RELEASE_WORKFLOW_SHA: \$\{\{ github\.sha \}\}/);
  assert.match(publisher, /run: bash scripts\/publish-github-release\.sh/);
  assert.doesNotMatch(publisher, /--generate-notes/);
});
