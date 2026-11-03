#!/usr/bin/env bash
# Type-check ratchet.
#
# mypy is strict and the production code carries a known baseline of errors.
# This gate fails when the error count grows above the recorded baseline, so
# the type-check leg of ci/required can actually fail — which it could not while
# the step ran with continue-on-error.
#
# When the count drops, the script prints the new number: lower the baseline in
# the same commit to ratchet the gate downwards.
set -euo pipefail

BASELINE_FILE="${BASELINE_FILE:-.github/mypy-baseline}"
TARGET="${TARGET:-src/agency_agents}"

if [[ ! -f "$BASELINE_FILE" ]]; then
  echo "baseline file not found: $BASELINE_FILE" >&2
  exit 1
fi

BASELINE="$(tr -dc '0-9' < "$BASELINE_FILE")"
OUTPUT="$(uv run mypy "$TARGET" 2>&1 || true)"
CURRENT="$(printf '%s\n' "$OUTPUT" | grep -c 'error:' || true)"

echo "mypy: current=${CURRENT} baseline=${BASELINE}"

if (( CURRENT > BASELINE )); then
  printf '%s\n' "$OUTPUT" | tail -40
  echo "::error::Type errors increased above the recorded baseline (${CURRENT} > ${BASELINE})."
  exit 1
fi

if (( CURRENT < BASELINE )); then
  echo "Type errors decreased. Lower ${BASELINE_FILE} to ${CURRENT} to ratchet the gate."
fi

echo "Type-check ratchet passed."
