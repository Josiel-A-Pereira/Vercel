# Releasing

Prepare each release in one pull request to `main`, updating `Cargo.toml`, `Cargo.lock`,
`npm/package.json`, and `CHANGELOG.md`. The versioned changelog section supplies the GitHub release notes.

## Prepare the notes

The latest versioned section in `CHANGELOG.md` must match `workspace.package.version` in `Cargo.toml`
exactly. Keep the existing linked heading with its date, or use a plain `## 1.2.3` heading with the
release's version. An optional `## Unreleased` section may precede it. No release-marker comments are required.

Follow agent-browser's release-note style. Use `### New Features`, `### Bug Fixes`, `### Improvements`,
`### Documentation`, and `### Breaking Changes` only when relevant, and omit empty sections. Give each
bullet a bold subject, a short explanation of the user-facing effect, and a PR reference. For example:

```markdown
- **Typing intervals** let callers pace keystrokes with `type --interval-ms` (#348).
```

Describe compatibility changes and any action users need to take under `### Breaking Changes`.
Review every merged PR and commit since the previous release tag for omitted changes, including docs-only work.

Add a mandatory `### Contributors` section with one `- @handle` bullet per human contributor.
Credit both PR authors and human coauthors since the previous release tag, including authors of docs-only
PRs and PRs without a separate change bullet. Resolve handles from GitHub PR metadata and GitHub-linked
commit identities; do not guess logins from names or email addresses. List each person once and exclude
bots and AI credits.

Apply this convention to new releases. Leave historical changelog sections unchanged.

## Validate before merging

From the repository root, preview and validate the current workspace version's notes:

```bash
node scripts/release-notes.js "$(node scripts/release-version.js)"
```

The command prints the selected section's authored notes without the version heading and exits nonzero
on a validation error. It requires Node.js but no dependency install and reads `CHANGELOG.md` by default.
Pass a file path after the version to read another changelog, or `-` to read stdin.

Contributor validation checks syntax and duplicates, not identity or completeness. Before merging,
check the human credits against the PR metadata and linked commit identities. The preview validates
the notes even for an already published version, so a legacy section without contributors fails this check.

Run the focused release tests:

```bash
node --test tests/release/*.test.js tests/npm/release-workflows.test.js
```

## Publish

Merging the release PR to `main` triggers `.github/workflows/release.yml` through the change to
`Cargo.toml`. If the current version already exists on both npm and GitHub, the workflow skips publication
without requiring historical notes to adopt the new format.

For a pending new release, preparation rejects missing, malformed, empty, or wrong-version notes,
including missing or invalid contributor entries, before creating the release tag.

The workflow builds and checks the native CLI and FFI archives, bundles the CLI executables into npm,
and publishes npm through trusted publishing (OIDC). The npm job runs in the `Release` environment,
so a reviewer approves it in the workflow run before it publishes.

The GitHub publisher passes the same authored changelog text to `gh release create` with `--notes-file`,
not `--generate-notes`. It publishes the archives, checksums, and provenance with the built-in
`GITHUB_TOKEN`. No extra release token or package-update commit is required.

## Retry a release

Use manual dispatch on `main` to retry an interrupted release. When the version already has a tag,
preparation validates and extracts `CHANGELOG.md` from that tag, not from a later `main` commit.
Publishing uses those tagged notes too. Editing the changelog on `main` cannot repair invalid notes
in an existing tag.

If npm already has the version but the GitHub release is missing, the workflow rebuilds the tagged
source and publishes the GitHub release without republishing npm. A rerun does not overwrite an
existing GitHub release body.

## Other package managers

The Homebrew formula lives in `Homebrew/homebrew-core` and builds from the release tag's source archive.
Homebrew's autobump automation detects the new tag and updates the formula, so the release needs no Homebrew step.
Homebrew availability can lag behind npm and GitHub publication.
There is no official Scoop package; do not advertise Scoop installation until `ScoopInstaller/Main` accepts one.
