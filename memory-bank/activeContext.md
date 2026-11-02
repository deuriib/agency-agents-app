## Active Context — Agency Agents

**State (2026-11-02)**: **PR #38 is open and CI-green.** It fixes the in-app updater
(which could never install: no release ever shipped `latest.json`), adds stable/beta
channels (#23), and completes the auto-install locale keys (#24). Not merged yet.

- **In review (PR #38, branch `fix/updater-pipeline`)**:
  - **Release pipeline**: signed updater artifacts in CI, a `manifest` job that builds
    and attaches `latest.json`, the Linux tarball regenerated after the Wayland repack,
    and `scripts/release.sh` uploading macOS artifacts, merging the `darwin-*` entries
    and publishing. The stale `agencyagents.app/updater.json` fallback is gone.
  - **Channels (#23)**: persisted `UpdateChannel` (stable | beta); beta follows the
    newest published prerelease and falls back to stable; the manifest URL is cached
    with the announced version; Settings has a real selector and re-checks on change.
  - **i18n (#24)**: the three missing consent keys in all 11 translations, channel
    labels/hints, dead key removal, and the `onlineHint` correction.
  - **Tooling**: `mise run verify-updater` (published-manifest check) and
    `mise run set-updater-secrets` (key-id-checked secret upload).
  - **Evidence**: 53 script tests, 16 new Rust tests, fmt/clippy/test, svelte-check 0,
    frontend build, actionlint; 8/8 CI checks green.
- **Landed in v0.6.0**: opt-in automatic installs (scheduler-owned), single-flight
  installs, renderer/ledger test repairs, eleven `autoInstallHint` corrections.
- **Landed in v0.5.1**: version bump plus the manifest-drift repair (all five manifests
  in lockstep). Its release notes claimed `antigravity-md`, which landed in v0.6.0.
- **Landed in v0.4.3**: multi-file renderers (Kimi, OpenClaw, Vibe), composite ledger
  hashes, automated release workflow.

**Current Focus**:
1. **Add the signing secrets** (`TAURI_SIGNING_PRIVATE_KEY` /
   `TAURI_SIGNING_PRIVATE_KEY_PASSWORD`) — `mise run set-updater-secrets` on the Mac.
2. **Merge PR #38** after human review; CI is green.
3. **Cut the next release**: merge the release-please PR → CI attaches the Linux/Windows
   assets and `latest.json` → `scripts/release.sh` on the Mac completes the `darwin-*`
   entries and publishes.
4. **Verify end-to-end**: install the previous version, update, and confirm
   `mise run verify-updater -- --check-urls`.
5. **Native review** of the new translations.

**Known Constraints**:
- The real install is unverified until a signed, published release exists.
- macOS still needs local notarization; CI cannot produce the DMGs.
- The v0.7.1 draft release is stranded; delete it once PR #38 lands.
- The beta rolling release `updater-beta` must never be marked "latest".

**Gates**:
- `mise run ci` (fmt, clippy, Rust tests, scripts, svelte-check, frontend build).
- `mise run verify-updater` after a release (all five platform keys).
- `cargo test` for backend validation.

---
## History (Obsolete)
v0.3.1 and prior contexts are now deprecated. Refer to `docs/release-notes/` for history.
