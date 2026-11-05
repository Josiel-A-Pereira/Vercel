# Releasing

Update the Cargo workspace version, lockfile, npm source version, and changelog in one pull request.
After it merges to `main`, changes to `Cargo.toml` trigger the existing Release workflow.
An unchanged published version skips publication. Manual dispatch remains available for retries.

Before creating a tag or building release artifacts, the workflow waits up to 60 minutes for CI, CodeQL,
and Supply Chain push runs on `main` to succeed for the exact release source commit. A failed or cancelled
run blocks the release. Missing or unfinished checks block it until they pass or the wait times out.
Retries of an existing tag check that tag's commit, even when `main` has advanced. Rerun failed checks
for that commit before retrying Release.

The workflow builds and checks the native CLI and FFI archives, bundles the CLI executables into npm,
publishes npm through trusted publishing (OIDC), and publishes GitHub archives, checksums, and provenance
with the built-in `GITHUB_TOKEN`. No extra release token or package-update commit is required.
The npm job runs in the `Release` environment, so a reviewer approves it in the workflow run before it publishes.

The Homebrew formula lives in `Homebrew/homebrew-core` and builds from the release tag's source archive.
Homebrew's autobump automation detects the new tag and updates the formula, so the release needs no Homebrew step.
Homebrew availability can lag behind npm and GitHub publication.
There is no official Scoop package; do not advertise Scoop installation until `ScoopInstaller/Main` accepts one.
