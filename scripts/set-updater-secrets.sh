#!/usr/bin/env bash
#
# set-updater-secrets.sh — load the updater signing key into GitHub Actions
# secrets without echoing it.
#
# Why: a release whose latest.json is signed by the wrong key fails
# verification on every client. This script checks the key id against the
# pubkey already embedded in src-tauri/tauri.conf.json before uploading, and
# pipes the value through stdin so it never reaches argv, a log, or the shell
# history.
#
# Usage:
#   scripts/set-updater-secrets.sh [--repo OWNER/REPO] [--key-file PATH]
#
# Key sources, in order:
#   1. --key-file PATH (default ~/.config/agency-agents-app/updater.key)
#   2. macOS Keychain: security find-generic-password -a agency-agents \
#        -s agency-agents-updater-key -w
# Password sources, in order:
#   1. TAURI_SIGNING_PRIVATE_KEY_PASSWORD
#   2. macOS Keychain: -s agency-agents-updater-key-pw
#   3. empty (only valid for an unencrypted key)
#
# Requires: gh (authenticated, repo scope), node, and on macOS `security`.
# Run it on the machine that holds the key — never paste the key into a chat,
# a ticket, or a shell argument.
#
# Exit codes: 0 = secrets uploaded, 1 = precondition/key mismatch, 2 = bad usage.

set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT"

REPO="deuriib/agency-agents-app"
KEY_FILE="${HOME}/.config/agency-agents-app/updater.key"

while [[ $# -gt 0 ]]; do
  case "$1" in
    --repo)
      REPO="${2:?--repo needs a value}"
      shift 2
      ;;
    --key-file)
      KEY_FILE="${2:?--key-file needs a value}"
      shift 2
      ;;
    -h | --help)
      sed -n '2,32p' "$0"
      exit 0
      ;;
    *)
      echo "unknown argument: $1 (try --help)" >&2
      exit 2
      ;;
  esac
done

command -v gh >/dev/null 2>&1 || {
  echo "✗ gh (GitHub CLI) is required — brew install gh && gh auth login" >&2
  exit 1
}
command -v node >/dev/null 2>&1 || {
  echo "✗ node is required to validate the key id" >&2
  exit 1
}
gh auth status >/dev/null 2>&1 || {
  echo "✗ gh is not authenticated — run 'gh auth login' (repo scope)" >&2
  exit 1
}

KEY=""
if [[ -f "$KEY_FILE" ]]; then
  KEY="$(<"$KEY_FILE")"
elif command -v security >/dev/null 2>&1; then
  KEY="$(security find-generic-password \
    -a "agency-agents" -s "agency-agents-updater-key" -w 2>/dev/null || true)"
fi
if [[ -z "$KEY" ]]; then
  echo "✗ no updater key found." >&2
  echo "  Looked for: $KEY_FILE" >&2
  echo "  and the macOS Keychain item 'agency-agents-updater-key'." >&2
  echo "  Pass --key-file PATH, or see docs/updater-release.md → Required secrets." >&2
  exit 1
fi

PASSWORD="${TAURI_SIGNING_PRIVATE_KEY_PASSWORD:-}"
if [[ -z "$PASSWORD" ]] && command -v security >/dev/null 2>&1; then
  PASSWORD="$(security find-generic-password \
    -a "agency-agents" -s "agency-agents-updater-key-pw" -w 2>/dev/null || true)"
fi

# No Keychain (Linux/Windows) and no env var: prompt once, without echo. This is
# the normal path when the key was generated on Linux for a CI-only release.
if [[ -z "$PASSWORD" && "$KEY" == *"minisign encrypted secret key"* ]]; then
  read -r -s -p "Updater key password: " PASSWORD
  echo
fi

# An encrypted key without its password produces a build that fails at signing
# time, after a long compile. Catch it now.
if [[ "$KEY" == *"minisign encrypted secret key"* && -z "$PASSWORD" ]]; then
  echo "✗ the key is encrypted but no password was provided." >&2
  echo "  Set TAURI_SIGNING_PRIVATE_KEY_PASSWORD or run interactively." >&2
  exit 1
fi

# Compare the minisign key ids (bytes 2..10 of each decoded blob). Only the ids
# are printed; the key itself goes in on stdin and never comes out.
printf '%s' "$KEY" | node -e '
const fs = require("fs");
const cfg = require("./src-tauri/tauri.conf.json");
const pub = Buffer.from(cfg.plugins.updater.pubkey, "base64")
  .toString("utf8")
  .trim()
  .split("\n")[1];
const priv = fs.readFileSync(0, "utf8");
const line = priv
  .split("\n")
  .find((candidate) => candidate.trim() && !candidate.startsWith("untrusted comment:"));
const id = (blob) => Buffer.from(blob, "base64").subarray(2, 10).toString("hex").toUpperCase();
const expected = id(pub);
const actual = line ? id(line) : "NONE";
console.log(`pubkey key id:  ${expected}`);
if (expected !== actual) {
  console.error(`private key id: ${actual}`);
  console.error("MISMATCH: this key cannot sign updates for the running apps.");
  process.exit(1);
}
console.log("key id matches the embedded pubkey");
' || {
  echo "✗ refusing to upload: the key does not match tauri.conf.json" >&2
  echo "  Rotate the pair and update the pubkey, or use the key that built it." >&2
  exit 1
}

printf '%s' "$KEY" | gh secret set TAURI_SIGNING_PRIVATE_KEY --repo "$REPO"
if [[ -n "$PASSWORD" ]]; then
  printf '%s' "$PASSWORD" | gh secret set TAURI_SIGNING_PRIVATE_KEY_PASSWORD --repo "$REPO"
else
  echo "▸ no key password found — leaving TAURI_SIGNING_PRIVATE_KEY_PASSWORD unset"
fi

unset KEY PASSWORD
echo "✓ updater signing secrets are set on $REPO:"
gh secret list --repo "$REPO"
