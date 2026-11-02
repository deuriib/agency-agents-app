# 261102_updater-pipeline

## Objective
Fix the in-app updater, which could never install anything, and close the two open
update issues: #23 (stable/beta channels) and #24 (auto-install locale keys).

## Outcome
- ✅ **Root causes fixed**:
  - No release ever shipped `latest.json` → `GET /releases/latest/download/latest.json`
    returned 404 and every check reported `installable: false`. New
    `scripts/updater-manifest.mjs` builds/merges the manifest; a `manifest` CI job
    attaches it to every release.
  - The fallback endpoint `agencyagents.app/updater.json` served a v0.3.2 manifest
    pointing at the upstream `msitarzewski` repo. Removed from `tauri.conf.json`.
  - `linux-build.yml` / `windows-build.yml` passed `createUpdaterArtifacts: false` and
    never uploaded `.sig`. Signing is enabled from
    `TAURI_SIGNING_PRIVATE_KEY`/`_PASSWORD` (env, never argv), every signature is
    attached, and a preflight fails fast when the key is missing.
  - The Linux AppImage is repacked after Tauri signs its updater tarball, so the
    signed payload contained the AppImage that crashes at EGL init. The tarball is
    now regenerated from the repacked AppImage and re-signed.
  - `scripts/release.sh` built DMGs but uploaded nothing. It now uploads macOS
    artifacts, merges the `darwin-*` entries, publishes the release, mirrors
    prereleases to `updater-beta`, and refuses to publish a manifest missing a
    platform.
- ✅ **Channels (#23)**: persisted `UpdateChannel { stable, beta }`; beta selects the
  newest published prerelease and falls back to stable; the manifest URL is resolved
  per check and cached with the announced version; Settings has a real selector and
  re-checks immediately after a change.
- ✅ **i18n (#24)**: the three missing auto-install consent keys are in all 11
  translations, plus `channelHint`/`channelStable`/`channelBeta`; dead
  `updates.stable`/`updates.noBeta` removed; `onlineHint` no longer claims the check
  reads `agencyagents.app/updater.json`.
- ✅ **Tooling**: `mise run verify-updater` (validates the published manifest, all
  five platform keys, empty signatures, non-https URLs, optional `--check-urls`) and
  `mise run set-updater-secrets` (key-id check + upload through stdin, never echoing
  the key).
- ✅ **Tests**: 53 script tests (37 manifest + 13 verifier + 3 existing), 16 new Rust
  tests, `cargo fmt`/`clippy`/`test`, `svelte-check` 0, production frontend build.
- ✅ PR #38 opened; all 8 CI checks green at the time of writing.

## Not yet verified (honest status)
- The real end-to-end install requires the signing secrets and a signed, published
  release. Everything up to that boundary is tested; the install itself is not.
- `TAURI_SIGNING_PRIVATE_KEY` and `TAURI_SIGNING_PRIVATE_KEY_PASSWORD` must be added
  by the maintainer.
- The new translations are AI-authored and need a native pass.

## Files Modified
- `src-tauri/src/commands/settings.rs` — `UpdateChannel`, persisted field, serde tests.
- `src-tauri/src/github/releases.rs` — `prerelease`, `fetch_channel_release`, shared
  fetch/parse path, beta tests.
- `src-tauri/src/commands/updater.rs` — channel-aware source, cached `manifest_url`,
  per-install endpoints, new tests.
- `src-tauri/src/github/mod.rs` — re-export the channel fetch.
- `src-tauri/Cargo.lock` — package version sync to 0.7.1.
- `src-tauri/tauri.conf.json` — removed the stale fallback endpoint.
- `src/lib/types.ts`, `src/lib/components/SettingsSectionUpdates.svelte` — channel
  selector + immediate re-check.
- `src/lib/i18n/locales/*.ts` — consent/channel keys, `onlineHint` correction, dead
  key removal.
- `.github/workflows/{linux-build,windows-build,release-please}.yml` — signing,
  `.sig` upload, manifest job, beta mirror.
- `scripts/release.sh` — macOS upload/merge/publish, partial-manifest guard.
- `scripts/updater-manifest.mjs` (+ tests) — manifest builder.
- `scripts/verify-updater-manifest.mjs` (+ tests) — published-manifest verifier.
- `scripts/set-updater-secrets.sh` — key-id-checked secret upload.
- `.mise.toml` — `verify-updater` and `set-updater-secrets` tasks.
- `docs/updater-release.md` — NEW runbook; `docs/BUILD.md` — pointed at it.
- `tools/release/publish-manifest.sh` — deleted (superseded).

## Patterns Applied
- **Manifest builder + verifier as a pair**: the producer validates every input and
  writes atomically; the verifier re-checks the published artifact with the same
  allowlist. A release is not done until both agree.
- **Pin the payload to the announced version**: the manifest URL is cached alongside
  the `Available` outcome, so a channel change between check and install cannot
  download a different release's bytes.
- **No partial publishes**: `release.sh` and the CI manifest job both fail closed when
  a platform is missing.
- **Secret handling**: key id is compared without printing the key; the value travels
  through stdin, never argv.

## Integration Points
- `release-please.yml` → `linux`/`windows` (signed assets) → `manifest` job (latest.json,
  beta mirror). `scripts/release.sh` completes the macOS entries and publishes.
- `run_check` reads `Settings::update_channel`; `PluginBackend` builds the updater with
  the cached `manifest_url` via `updater_builder().endpoints(...)`.
- `.mise.toml` → `verify-updater` (post-release check) and `set-updater-secrets`
  (pre-release setup).

## Architectural Decisions
- See `decisions.md` → 2026-11-02. The manifest is a release asset; channels select a
  release stream; the draft-then-human-publish model is retained.

## Notes
- The v0.7.1 draft release is stranded (no `latest.json`); delete it once this lands.
- A published release still requires `scripts/release.sh` on the Mac for Apple
  notarization and for completing the `darwin-*` manifest entries.
- Beta uses the rolling `updater-beta` release; it must never be marked "latest".
