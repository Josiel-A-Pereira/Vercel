#!/usr/bin/env bash
set -euo pipefail
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
if [[ ! "${RELEASE_SOURCE_SHA:-}" =~ ^[0-9a-f]{40}$ ]] || [[ ! "${RELEASE_WORKFLOW_SHA:-}" =~ ^[0-9a-f]{40}$ ]]; then
  echo "Release source and workflow must be full commit SHAs" >&2
  exit 1
fi
if ! git -c safe.directory="$PWD" merge-base --is-ancestor "$RELEASE_SOURCE_SHA" "$RELEASE_WORKFLOW_SHA"; then
  echo "Release source must belong to the triggering main history" >&2
  exit 1
fi
SOURCE_VERSION=$(git show "${RELEASE_SOURCE_SHA}:Cargo.toml" | node "${SCRIPT_DIR}/release-version.js" -)
if [ "$SOURCE_VERSION" != "${RELEASE_VERSION:-}" ]; then
  echo "Release source version does not match the version being published" >&2
  exit 1
fi
NOTES_FILE=$(mktemp "${RUNNER_TEMP:-${TMPDIR:-/tmp}}/agent-desktop-release-notes.XXXXXX")
trap 'rm -f "$NOTES_FILE"' EXIT
git show "${RELEASE_SOURCE_SHA}:CHANGELOG.md" | node "${SCRIPT_DIR}/release-notes.js" "$RELEASE_VERSION" - > "$NOTES_FILE"
TAG_NAME="v${RELEASE_VERSION}"
if ! gh release view "$TAG_NAME" >/dev/null 2>&1; then
  gh release create "$TAG_NAME" \
    --verify-tag \
    --title "$TAG_NAME" \
    --notes-file "$NOTES_FILE"
fi
gh release upload "$TAG_NAME" \
  ./*.tar.gz ./*.zip checksums.txt \
  --clobber
