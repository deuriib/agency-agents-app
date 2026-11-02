#!/usr/bin/env bash
#
# release.sh — signed + notarized macOS build of Agency Agents, published to the
# GitHub release the app updates itself from.
#
# What it does, in order:
#   1. builds and notarizes both Mac arches (DMG + updater `.app.tar.gz`)
#   2. uploads every artifact + signature to the release-please draft release
#   3. merges the `darwin-*` entries into `latest.json` and uploads it
#   4. publishes the release (unless NO_PUBLISH=1)
#   5. mirrors `latest.json` to the `updater-beta` channel for prereleases
#
# Requires the GitHub CLI, authenticated with `repo` scope: the release IS the
# update channel, so a build that cannot be attached to it is a wasted build.
# Checked before anything compiles.
#
# Secrets live in the macOS Keychain, NOT in env files or shell history. This
# script pulls them out at build time and hands them to Tauri (which signs with
# your Developer ID, submits to Apple's notary service, and staples the ticket).
#
# ── ONE-TIME SETUP ──────────────────────────────────────────────────────────
# 0. GitHub CLI: `brew install gh && gh auth login` (https, or an SSH token with
#    `repo` scope). Verify with `gh auth status`.
#
# 1. App-specific password (NOT your iCloud password): create one at
#    https://appleid.apple.com → Sign-In and Security → App-Specific Passwords.
#    Then store it in the Keychain:
#
#       security add-generic-password \
#         -a "msitarzewski@mac.com" -s "agency-agents-notary" -U -w
#       # (paste the app-specific password when prompted)
#
#    Then save the same password as a notarytool profile (it prompts for the
#    password, so it never lands on a command line or in shell history). The
#    script uses this profile to notarize + staple each .dmg after Tauri builds it:
#       xcrun notarytool store-credentials "agency-agents" \
#         --apple-id "msitarzewski@mac.com" --team-id "7JQGQ7CRH8"
#
# 2. Tauri updater signing key (minisign — signs the auto-update artifacts so
#    the app trusts its own updates; separate from Apple signing). Store the
#    PRIVATE key + its password in the Keychain too:
#
#       security add-generic-password -a "agency-agents" -s "agency-agents-updater-key"     -U -w   # paste private key contents
#       security add-generic-password -a "agency-agents" -s "agency-agents-updater-key-pw"  -U -w   # paste its password
#
# After setup, just run:  ./scripts/release.sh
# ────────────────────────────────────────────────────────────────────────────
set -euo pipefail

# Non-secret identifiers (safe to keep in the script).
export APPLE_ID="msitarzewski@mac.com"
export APPLE_TEAM_ID="7JQGQ7CRH8"   # from "Developer ID Application: … (7JQGQ7CRH8)"

# Release target: the draft release-please created for this version, on the repo
# the app's updater endpoint points at.
REPO_SLUG="${REPO_SLUG:-deuriib/agency-agents-app}"
BETA_TAG="updater-beta"             # rolling prerelease channel (#23)
# Release assets must not contain spaces: the manifest URL is built from the
# asset name, and while it is percent-encoded, a space-free name keeps the
# tarball portable for anyone downloading it by hand.
PRODUCT_SLUG="Agency.Agents"

NOTARY_SERVICE="agency-agents-notary"
UPDATER_KEY_SERVICE="agency-agents-updater-key"
UPDATER_KEY_PW_SERVICE="agency-agents-updater-key-pw"

kc() { security find-generic-password -a "$1" -s "$2" -w 2>/dev/null || true; }

NOTARY_PROFILE="${NOTARY_PROFILE:-agency-agents}"   # notarytool keychain profile (SETUP step 1)

# ── Apple notarization password (from Keychain) ──
APPLE_PASSWORD="$(kc "$APPLE_ID" "$NOTARY_SERVICE")"
if [[ -z "$APPLE_PASSWORD" ]]; then
  echo "✗ No notarization password in the Keychain (service '$NOTARY_SERVICE')." >&2
  echo "  Run the ONE-TIME SETUP at the top of scripts/release.sh, then retry." >&2
  exit 1
fi
export APPLE_PASSWORD

# ── Tauri updater signing key (from Keychain) ──
# tauri.conf has createUpdaterArtifacts: true, so a normal build signs the
# auto-update bundle and REQUIRES the minisign private key. SKIP_UPDATER=1
# turns the updater artifact OFF for this build (config override) so the run
# exits clean with just the signed+notarized .app/.dmg.
# A --config flag forces the tauri CLI to fully re-resolve the config, which
# auto-merges tauri.macos.conf.json (where macOSPrivateApi:true lives) into the
# config the build-script allowlist check reads. Without ANY --config, that
# check sees only base tauri.conf.json (no macOSPrivateApi) and rejects the
# macos-private-api feature from the [target.macos] block — see tauri#11142.
# (This is why every SKIP_UPDATER build worked but the updater-on path did not.)
BUILD_ARGS=(--config '{"app":{"macOSPrivateApi":true}}')
if [[ "${SKIP_UPDATER:-0}" == "1" ]]; then
  echo "▸ SKIP_UPDATER=1 — building WITHOUT updater artifacts."
  BUILD_ARGS+=(--config '{"bundle":{"createUpdaterArtifacts":false}}')
else
  TAURI_SIGNING_PRIVATE_KEY="$(kc "agency-agents" "$UPDATER_KEY_SERVICE")"
  TAURI_SIGNING_PRIVATE_KEY_PASSWORD="$(kc "agency-agents" "$UPDATER_KEY_PW_SERVICE")"
  if [[ -z "$TAURI_SIGNING_PRIVATE_KEY" ]]; then
    echo "✗ No updater signing key in the Keychain (service '$UPDATER_KEY_SERVICE')." >&2
    echo "  Store it (see SETUP) or run with SKIP_UPDATER=1 to skip update artifacts." >&2
    exit 1
  fi
  export TAURI_SIGNING_PRIVATE_KEY TAURI_SIGNING_PRIVATE_KEY_PASSWORD
fi

# ── GitHub CLI (required to publish) ──
# Checked now, not after notarization: `gh` is how the DMGs, updater tarballs,
# signatures and `latest.json` reach the release, and the updater fetches its
# manifest from that same release. A build that cannot be uploaded leaves the
# app pointing at a manifest that does not exist (the "always up to date" bug).
if ! command -v gh >/dev/null 2>&1; then
  echo "✗ 'gh' (GitHub CLI) is not installed — 'brew install gh', then 'gh auth login'." >&2
  exit 1
fi
if ! gh auth status >/dev/null 2>&1; then
  echo "✗ 'gh' is not authenticated — run 'gh auth login' (needs 'repo' scope to" >&2
  echo "  upload release assets). Check with 'gh auth status'." >&2
  exit 1
fi

# Build BOTH macOS architectures so we ship Apple Silicon + Intel DMGs. Each
# build signs with your Developer ID, notarizes, and staples. The HOST arch
# builds natively into target/release/bundle/; other arches cross-compile into
# target/<triple>/release/bundle/. Override the set with e.g.
# RELEASE_TARGETS="aarch64-apple-darwin x86_64-apple-darwin".
#
# ⚠️ macOS BETA TOOLCHAIN GOTCHA: on a beta macOS/Xcode (e.g. Tahoe/26+), this
# build can fail with "can't find crate for <proc-macro>" (ctor_proc_macro,
# tauri_macros, serde_with_macros, …). Root cause: the MACOSX_DEPLOYMENT_TARGET
# the Tauri CLI sets (from minimumSystemVersion) breaks FRESH proc-macro
# compilation under the beta toolchain. Stable macOS / CI are unaffected.
# Recovery (pre-compile proc-macros without that env var, then let Tauri reuse
# the cache to bundle/sign/notarize) is documented in docs/BUILD.md →
# "Troubleshooting: proc-macro 'can't find crate' on a beta macOS".
# Tauri notarizes + staples the .app, then wraps it in a .dmg it signs but does
# NOT notarize. Apple's guidance is to notarize the outermost container, so each
# .dmg is submitted + stapled below. Check the profile now (it also surfaces an
# expired Apple agreement as a 403) rather than after a long build.
if ! xcrun notarytool history --keychain-profile "$NOTARY_PROFILE" >/dev/null 2>&1; then
  echo "✗ notarytool profile '$NOTARY_PROFILE' is missing or Apple rejected it." >&2
  echo "  Create it (SETUP step 1), or check: xcrun notarytool history --keychain-profile $NOTARY_PROFILE" >&2
  exit 1
fi
VERSION="$(node -p "require('./src-tauri/tauri.conf.json').version")"
# Versioned once, used by the notarizer, the upload and the beta-channel check.
# A prerelease version (0.8.0-beta.1) is published as a prerelease so the stable
# endpoint keeps pointing at the last stable release.
TAG="v${VERSION}"

notarize_dmg() {
  local dmg
  dmg="$(ls "$1"/*_"${VERSION}"_*.dmg 2>/dev/null | head -1)"
  if [[ -z "$dmg" ]]; then echo "✗ No ${VERSION} .dmg in $1" >&2; exit 1; fi
  echo "▸ Notarizing $(basename "$dmg")…"
  xcrun notarytool submit "$dmg" --keychain-profile "$NOTARY_PROFILE" --wait
  xcrun stapler staple "$dmg"
  spctl --assess --type install -v "$dmg"
}

read -r -a TARGETS <<< "${RELEASE_TARGETS:-aarch64-apple-darwin x86_64-apple-darwin}"
HOST_TRIPLE="$(rustc -vV | awk '/^host:/{print $2}')"
DMG_DIRS=()

for target in "${TARGETS[@]}"; do
  if [[ "$target" == "$HOST_TRIPLE" ]]; then
    echo "▸ Building signed + notarized Agency Agents for ${target} (native, Team $APPLE_TEAM_ID)…"
    npm run tauri build -- ${BUILD_ARGS[@]+"${BUILD_ARGS[@]}"}
    DMG_DIRS+=("src-tauri/target/release/bundle/dmg/")
    notarize_dmg "src-tauri/target/release/bundle/dmg"
  else
    echo "▸ Building signed + notarized Agency Agents for ${target} (cross, Team $APPLE_TEAM_ID)…"
    npm run tauri build -- --target "$target" ${BUILD_ARGS[@]+"${BUILD_ARGS[@]}"}
    DMG_DIRS+=("src-tauri/target/${target}/release/bundle/dmg/")
    notarize_dmg "src-tauri/target/${target}/release/bundle/dmg"
  fi
done

echo
echo "✓ Signed, notarized + stapled DMGs:"
for d in "${DMG_DIRS[@]}"; do echo "    $d"; done

# ── Collect, upload, publish ────────────────────────────────────────────────
if [[ "${SKIP_UPDATER:-0}" == "1" ]]; then
  echo
  echo "▸ SKIP_UPDATER=1 — this was a local build. No updater artifacts exist, so"
  echo "  nothing was uploaded to $REPO_SLUG:$TAG. Re-run without SKIP_UPDATER to publish."
  exit 0
fi

# Everything is staged under a temp dir first: a half-collected set must never
# reach the release, where clients would fetch a manifest pointing at a missing
# file. The trap cleans up on any exit path.
STAGE="$(mktemp -d)"
trap 'rm -rf "$STAGE"' EXIT

# Rename the updater payloads to a space-free, versioned name so both arches can
# sit on one release and the manifest URL needs no guessing. The manifest only
# accepts the five Tauri platform keys, so map target -> key here.
MANIFEST_ARGS=()
shopt -s nullglob
for target in "${TARGETS[@]}"; do
  case "$target" in
    aarch64-apple-darwin) ARCH_LABEL="aarch64"; PLATFORM_KEY="darwin-aarch64" ;;
    x86_64-apple-darwin)  ARCH_LABEL="x64";     PLATFORM_KEY="darwin-x86_64" ;;
    *)
      echo "✗ Unsupported macOS target '$target' — add its updater platform key here." >&2
      exit 1
      ;;
  esac

  if [[ "$target" == "$HOST_TRIPLE" ]]; then
    BUNDLE_DIR="src-tauri/target/release/bundle"
  else
    BUNDLE_DIR="src-tauri/target/${target}/release/bundle"
  fi

  for dmg in "$BUNDLE_DIR"/dmg/*_"${VERSION}"_*.dmg; do
    cp -p "$dmg" "$STAGE/"
  done

  # Tauri emits `Agency Agents.app.tar.gz` (a gzipped .app, never the .dmg) plus
  # its minisign signature during a normal build; both must exist or the update
  # cannot be verified by the running app.
  app_tgz=( "$BUNDLE_DIR"/macos/*.app.tar.gz )
  if [[ ${#app_tgz[@]} -ne 1 ]]; then
    echo "✗ Expected one updater tarball in $BUNDLE_DIR/macos, found ${#app_tgz[@]}." >&2
    echo "  Build without SKIP_UPDATER=1 and check the updater signing key." >&2
    exit 1
  fi
  if [[ ! -s "${app_tgz[0]}.sig" ]]; then
    echo "✗ Missing signature next to ${app_tgz[0]} — the build did not sign the updater artifact." >&2
    exit 1
  fi
  ASSET_NAME="${PRODUCT_SLUG}_${VERSION}_${ARCH_LABEL}.app.tar.gz"
  cp -p "${app_tgz[0]}" "$STAGE/$ASSET_NAME"
  cp -p "${app_tgz[0]}.sig" "$STAGE/$ASSET_NAME.sig"
  MANIFEST_ARGS+=(--asset "${PLATFORM_KEY}=${ASSET_NAME}")
  echo "▸ Staged $ASSET_NAME (+ .sig) for ${PLATFORM_KEY}"
done
shopt -u nullglob

if ! gh release view "$TAG" --repo "$REPO_SLUG" >/dev/null 2>&1; then
  echo "✗ No release '$TAG' on $REPO_SLUG." >&2
  echo "  Merge the release-please release PR first: it creates the draft, and CI" >&2
  echo "  attaches the Linux + Windows artifacts to it before macOS publishes." >&2
  exit 1
fi

echo "▸ Uploading DMGs + updater tarballs to $TAG (draft)…"
gh release upload "$TAG" \
  "$STAGE"/*.dmg \
  "$STAGE"/*.app.tar.gz \
  "$STAGE"/*.app.tar.gz.sig \
  --repo "$REPO_SLUG" \
  --clobber

# The manifest is rebuilt from the one CI already attached (Linux + Windows
# entries) with --merge, so a macOS-only run cannot drop the other platforms.
MANIFEST_DIR="$STAGE/manifest"
mkdir -p "$MANIFEST_DIR"
if gh release view "$TAG" --repo "$REPO_SLUG" --json assets --jq '.assets[].name' \
    | grep -Fqx 'latest.json'; then
  gh release download "$TAG" --repo "$REPO_SLUG" --pattern latest.json \
    --dir "$MANIFEST_DIR" --clobber
  echo "▸ Merging darwin entries into the existing latest.json"
else
  echo "▸ No latest.json on $TAG yet — creating it with the darwin entries only"
fi

# The updater shows the release body as the update notes.
gh release view "$TAG" --repo "$REPO_SLUG" --json body --jq .body > "$STAGE/notes.md"

node scripts/updater-manifest.mjs \
  --version "$VERSION" \
  --notes-file "$STAGE/notes.md" \
  --pub-date "$(date -u +%Y-%m-%dT%H:%M:%SZ)" \
  --base-url "https://github.com/${REPO_SLUG}/releases/download/${TAG}" \
  --assets-dir "$STAGE" \
  ${MANIFEST_ARGS[@]+"${MANIFEST_ARGS[@]}"} \
  --merge "$MANIFEST_DIR/latest.json" \
  --out "$STAGE/latest.json"

gh release upload "$TAG" "$STAGE/latest.json" --repo "$REPO_SLUG" --clobber
echo "▸ latest.json attached:"
cat "$STAGE/latest.json"
echo

if [[ "${NO_PUBLISH:-0}" == "1" ]]; then
  echo "▸ NO_PUBLISH=1 — $TAG stays a draft. Publish later with:"
  echo "    gh release edit $TAG --draft=false"
  exit 0
fi

# A published release must be installable on every platform it claims to
# support. A darwin-only manifest silently strands Linux and Windows clients on
# the previous version (the plugin reports "no update for this target"), which
# is the same class of bug this pipeline exists to prevent. The CI manifest job
# contributes those entries; wait for it to finish. A deliberate macOS-only cut
# can opt out with ALLOW_PARTIAL_MANIFEST=1.
MISSING_PLATFORMS=""
for key in linux-x86_64 windows-x86_64 windows-aarch64; do
  if ! node -e 'const m=require(process.argv[1]); process.exit(m.platforms[process.argv[2]] ? 0 : 1)' \
      "$STAGE/latest.json" "$key"; then
    MISSING_PLATFORMS+=" $key"
  fi
done
if [[ -n "$MISSING_PLATFORMS" ]]; then
  if [[ "${ALLOW_PARTIAL_MANIFEST:-0}" == "1" ]]; then
    echo "▸ ALLOW_PARTIAL_MANIFEST=1 — publishing without:${MISSING_PLATFORMS}"
  else
    echo "✗ latest.json is missing:${MISSING_PLATFORMS}" >&2
    echo "  Wait for the CI manifest job (release-please.yml) and re-run, or set" >&2
    echo "  ALLOW_PARTIAL_MANIFEST=1 if this is a deliberate macOS-only release." >&2
    exit 1
  fi
fi

# A prerelease must never become "latest": the stable endpoint is
# releases/latest/download/latest.json, and GitHub picks it from the newest
# non-draft, non-prerelease release. release-please already sets this, but the
# flag is cheap insurance against publishing a beta to every stable client.
if [[ "$VERSION" == *-* ]]; then
  gh release edit "$TAG" --repo "$REPO_SLUG" --prerelease
fi

gh release edit "$TAG" --repo "$REPO_SLUG" --draft=false
echo "✓ Published $TAG → https://github.com/${REPO_SLUG}/releases/tag/${TAG}"

# ── Beta channel (#23) ──
# `updater-beta` is a long-lived release whose only asset is the newest
# prerelease manifest; beta clients fetch that fixed URL. Mirror only after
# publishing — a draft's assets 404 for anonymous clients, so mirroring earlier
# would hand beta users a manifest pointing at nothing.
if [[ "$VERSION" == *-* ]]; then
  if ! gh release view "$BETA_TAG" --repo "$REPO_SLUG" >/dev/null 2>&1; then
    gh release create "$BETA_TAG" \
      --repo "$REPO_SLUG" \
      --title "Updater beta channel" \
      --notes "Rolling manifest for Agency Agents prereleases. Overwritten by every beta release; the stable channel lives at releases/latest/download/latest.json." \
      --prerelease \
      --latest=false
  fi
  gh release upload "$BETA_TAG" "$STAGE/latest.json" --repo "$REPO_SLUG" --clobber
  echo "✓ Mirrored latest.json to the $BETA_TAG beta channel."
fi
