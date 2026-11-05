#!/usr/bin/env bash
set -euo pipefail
SOURCE_SHA=${1:?Release source commit is required}
if [[ ! "$SOURCE_SHA" =~ ^[0-9a-f]{40}$ ]]; then
  echo "Release source must be a full commit SHA" >&2
  exit 1
fi
while [ "$SECONDS" -lt 3600 ]; do
  READY=true
  for WORKFLOW in ci.yml codeql.yml supply-chain.yml; do
    STATE=$(gh api --method GET "repos/${GH_REPO}/actions/workflows/${WORKFLOW}/runs" \
      -f head_sha="$SOURCE_SHA" -f branch=main -f event=push -f per_page=1 \
      --jq '.workflow_runs[0] | if .status == "completed" then "completed \(.conclusion)" else .status // "missing" end')
    echo "${WORKFLOW} at ${SOURCE_SHA}: ${STATE}"
    case "$STATE" in
      'completed success') ;;
      missing|queued|in_progress|waiting|pending|requested) READY=false ;;
      *)
        echo "Release blocked by ${WORKFLOW}: ${STATE}" >&2
        exit 1
        ;;
    esac
  done
  if [ "$READY" = true ]; then
    exit 0
  fi
  sleep 30
done
echo "Timed out waiting for release checks at ${SOURCE_SHA}" >&2
exit 1
