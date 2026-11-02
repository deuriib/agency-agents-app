# Project Progress — Agency Agents

## Version 0.8.x (Current)
- [ ] **PR #39 (`chore(main): release 0.8.0`)** opened by release-please. Do not merge
  until the signing secrets exist: the Linux/Windows preflight fails without them.
- [ ] Set `TAURI_SIGNING_PRIVATE_KEY` and `TAURI_SIGNING_PRIVATE_KEY_PASSWORD`
  (`mise run set-updater-secrets` on the Mac).
- [ ] Merge PR #39 → `v0.8.0` draft → CI attaches signed Linux/Windows payloads and
  `latest.json`.
- [ ] Publish with `./scripts/release.sh` (macOS notarization + `darwin-*` entries).
- [ ] Verify with `mise run verify-updater -- --check-urls`, then update an installed
  0.7.x build end-to-end.
- [ ] Delete the stranded `v0.7.1` draft.
- [ ] Native review of the new translations.

## Version 0.7.x (Completed)
- [x] **PR #38 (merged `a453bd2`)**: updater installable end-to-end (`latest.json` +
  signing + publish), stable/beta channels (#23), auto-install locale keys (#24), and
  the `verify-updater` / `set-updater-secrets` tasks.

### Pending for the next milestone
- [ ] Manual smoke test of auto-install on a signed build.
- [ ] Implement multi-file reconcile logic (detect drift across multiple files).
- [ ] Implement multi-file uninstall logic.
- [ ] Enable Aider/Windsurf roster aggregation.
- [ ] Run full parity tests for multi-file outputs.

## Version 0.6.x (Completed)
- [x] v0.6.0: Opt-in automatic installs (scheduler-owned background download → verify → install), plus the six backend test repairs and eleven i18n hint corrections.

## Version 0.5.x (Completed)
- [x] v0.5.0: Version bump and release notes setup.
- [x] v0.5.1: Manifest-drift repair; the `antigravity-md` registration its notes claimed actually landed in v0.6.0.

## Version 0.4.x (Completed)
- [x] v0.4.0: Release workflow automation.
- [x] v0.4.1: Repository rebranding to deuriib.
- [x] v0.4.2: Default catalog repo update.
- [x] v0.4.3: Multi-file renderers (Kimi, OpenClaw, Vibe) and composite ledger hashes.
## Version 0.3.x (Completed)
- [x] v0.3.1: Platform fixes (WebView2, console flash, AppArmor).
- [x] v0.3.0: Runbooks headline feature.
