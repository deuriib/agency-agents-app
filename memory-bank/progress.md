# Project Progress — Agency Agents

## Version 0.9.x (Current)
- [x] **PR #61 (merged `1ab9453`)**: Rules integration, milestone M1 of issue #29.
  Native Rust compilation of the four `agency-rules` renderers (32/32 byte-identical
  to a fresh upstream compile), a second managed corpus with its own index and
  refresh, ledger tracking with a domain discriminator, OpenCode plugin registration,
  and a Rules workspace in all 12 locales. See
  [261103_rules-integration.md](tasks/2026-11/261103_rules-integration.md).
- [x] Fixed `atomic_write` (missing `flush()` before `sync_all` silently truncated on
  a full disk) and `load_ledger` (any read error became "nothing installed").
- [x] **v0.9.0** released by release-please (`00b9576`, PR #67).
- [ ] **Publish v0.9.0**: it is still a draft with Linux assets only. Missing
  `latest.json`, Windows artifacts and the notarized macOS payload. Needs the signing
  secrets and a local `./scripts/release.sh`, then `mise run verify-updater`.
- [ ] Native review of the 12-locale Rules strings.
- [ ] Triage the #61 review follow-ups (#62–#72); suggested order: #72 → #68 → #62 →
  #66, with #63/#65/#69 independent.

## Version 0.8.x (Completed)
- [x] 0.8.0 → 0.8.4 shipped: updater installable end-to-end (`latest.json` + signing +
  channels), auto-install locale keys, and the `verify-updater` /
  `set-updater-secrets` tasks.
- [x] 0.8.3/0.8.4: catalog snapshot carries `strategy/` and `divisions.json`;
  `IMPLEMENTED_FORMATS` read from a shared JSON (#54, #59).
- [ ] Each 0.8.x release still needs the macOS publish step performed locally; the
  draft left behind by CI only holds the Linux payloads.

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
