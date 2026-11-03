#!/usr/bin/env bash
# Apply scripts/github/ruleset.json to the repository through the GitHub API.
#
# Idempotent: updates the ruleset if one with the same name exists, creates it
# otherwise. Requires a token with administration permission on the repository.
set -euo pipefail

REPO="${1:-$(gh repo view --json nameWithOwner --jq .nameWithOwner)}"
NAME="main-protection"
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
RULESET_FILE="${SCRIPT_DIR}/ruleset.json"

if [[ ! -f "$RULESET_FILE" ]]; then
  echo "ruleset file not found: $RULESET_FILE" >&2
  exit 1
fi

existing_id="$(gh api "repos/${REPO}/rulesets" --jq ".[] | select(.name == \"${NAME}\") | .id" || true)"

if [[ -n "$existing_id" ]]; then
  gh api -X PUT "repos/${REPO}/rulesets/${existing_id}" --input "$RULESET_FILE" >/dev/null
  echo "updated ruleset '${NAME}' (id ${existing_id}) on ${REPO}"
else
  gh api -X POST "repos/${REPO}/rulesets" --input "$RULESET_FILE" >/dev/null
  echo "created ruleset '${NAME}' on ${REPO}"
fi

gh api "repos/${REPO}/rulesets" \
  --jq '.[] | "\(.id)\t\(.name)\t\(.enforcement)"'
