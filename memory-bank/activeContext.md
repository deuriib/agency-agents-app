## Active Context — Agency Agents

**State (2026-11-02)**: **PR #38 is merged** (`a453bd2`). It fixes the in-app updater
(which could never install: no release ever shipped `latest.json`), adds stable/beta
channels (#23), and completes the auto-install locale keys (#24). `release-please` has
already opened **PR #39 (`chore(main): release 0.8.0`)**.

- **Merged in PR #38 (`a453bd2`)**:
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
    frontend build, actionlint; 8/8 CI checks green before merge.
- **Landed in v0.6.0**: opt-in automatic installs (scheduler-owned), single-flight
  installs, renderer/ledger test repairs, eleven `autoInstallHint` corrections.
- **Landed in v0.5.1**: version bump plus the manifest-drift repair (all five manifests
  in lockstep). Its release notes claimed `antigravity-md`, which landed in v0.6.0.
- **Landed in v0.4.3**: multi-file renderers (Kimi, OpenClaw, Vibe), composite ledger
  hashes, automated release workflow.

**Current Focus**:
1. **Set the signing secrets BEFORE merging PR #39.** The release jobs fail the preflight
   without them: `mise run set-updater-secrets` on the Mac (key id is checked against
   the embedded pubkey).
2. **Merge PR #39 (`release 0.8.0`).** release-please creates the `v0.8.0` draft, then CI
   signs and attaches the Linux/Windows artifacts and `latest.json`.
3. **Publish from the Mac**: `./scripts/release.sh` builds/notarizes macOS, uploads the
   DMGs + `.app.tar.gz` + `.sig`, merges the `darwin-*` entries, and publishes.
4. **Verify end-to-end**: install the previous version, update, and run
   `mise run verify-updater -- --check-urls`.
5. **Native review** of the new translations.
6. **Delete the stranded `v0.7.1` draft** (never published, no manifest).

**Known Constraints**:
- The real install is unverified until a signed, published release exists.
- macOS still needs local notarization; CI cannot produce the DMGs.
- `gh secret list` currently shows no `TAURI_SIGNING_*` secrets — the release pipeline is
  blocked on that, not on code.
- The beta rolling release `updater-beta` must never be marked "latest".

**Gates**:
- `mise run ci` (fmt, clippy, Rust tests, scripts, svelte-check, frontend build).
- `mise run verify-updater` after a release (all five platform keys).
- `cargo test` for backend validation.

---
## History (Obsolete)
v0.3.1 and prior contexts are now deprecated. Refer to `docs/release-notes/` for history.
