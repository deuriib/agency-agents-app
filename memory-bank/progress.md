# Project Progress — Agency Agents

## Version 0.12.x (Current)
- [x] **PR #102 (merged `3ba967d`)**: the updater stops promising what it cannot deliver.
  `.deb` / `.rpm` builds are told they cannot self-update (#56) — a pure bundle-type gate
  refuses before the download, `inAppInstall` + `packageDownload` carry the capability in
  the payload, and the card offers the release's own installer; `InvalidUpdaterFormat`
  and `SelfUpdateUnsupported` can no longer surface as "Network error". Release notes are
  flattened to plain text **before** truncation (#57), so a link or URL cannot be cut in
  half. The retired `agencyagents.app` host is gone from the CSP, the network panel and
  all 12 locales (#47). 596 → 626 Rust tests. See
  [261104_updater-package-installs.md](tasks/2026-11/261104_updater-package-installs.md).
- [x] **0.12.0** (`5234cca`, PR #95): rules install through the destinations grid (#94).
- [x] **0.11.0** (`d63c7bb`, PR #88): multi-line rule frontmatter compiles as block
  scalars, which lifts the C11 install guard.
- [x] **0.10.0** (`4b52142`, PR #86): the Antigravity app is detected via
  `.gemini/antigravity` (#69), so a desktop-app-only user is no longer invisible.
- [x] **0.12.1** (`e924e09`, published 2026-11-04): PR #102. The updater stops promising
  what it cannot deliver — bundle-type gate, honest installer errors, plain-text release
  notes, retired host removed.
- [ ] **Publish the macOS payload.** The only remaining release gap: `latest.json` carries
  `linux-x86_64`, `windows-x86_64` and `windows-aarch64`, but **`darwin-aarch64` /
  `darwin-x86_64` are missing**, so macOS cannot update and `mise run verify-updater`
  returns **FAIL**. Needs a local `./scripts/release.sh` (CI cannot notarize), then
  `mise run verify-updater -- --check-urls`. Releases are otherwise **published**, not
  drafts — 0.8.1 forward.
- [ ] Native review of the 12-locale strings: Rules, plus the three `updates.*` keys
  added in #102.
- [ ] Decide **#56 option B** (per-installer manifests + a signing step for `.deb` /
  `.rpm`) or accept download-only Linux packages permanently. `activeContext.md` records
  the current answer as a constraint, not a bug.
- [ ] Remaining #61 follow-ups: #64 (AppHandle harness), #65 (parity guard: stale vs
  missing), #70, #71, #76, #98. #62, #63, #66, #68, #69 and #72 have shipped.

## Version 0.9.x (Completed)
- [x] **PR #61 (merged `1ab9453`)**: Rules integration, milestone M1 of issue #29.
  Native Rust compilation of the four `agency-rules` renderers (32/32 byte-identical
  to a fresh upstream compile), a second managed corpus with its own index and
  refresh, ledger tracking with a domain discriminator, OpenCode plugin registration,
  and a Rules workspace in all 12 locales. See
  [261103_rules-integration.md](tasks/2026-11/261103_rules-integration.md).
- [x] Fixed `atomic_write` (missing `flush()` before `sync_all` silently truncated on
  a full disk) and `load_ledger` (any read error became "nothing installed").
- [x] **v0.9.0** released by release-please (`00b9576`, PR #67).
- [x] 0.9.1 → 0.9.3: Apple identity from the environment (#74), `{slug}` + agent tool ids
  for rule harnesses (#78), Rules rendered through the shared workspace shell (#83).

## Version 0.8.x (Completed)
- [x] 0.8.0 → 0.8.4 shipped: updater installable end-to-end (`latest.json` + signing +
  channels), auto-install locale keys, and the `verify-updater` /
  `set-updater-secrets` tasks.
- [x] 0.8.3/0.8.4: catalog snapshot carries `strategy/` and `divisions.json`;
  `IMPLEMENTED_FORMATS` read from a shared JSON (#54, #59).
- [ ] **Every release still needs the macOS publish step run locally.** CI publishes the
  Linux and Windows payloads automatically (0.8.1 → 0.12.1 are all published), so this is
  not a draft problem — it is the missing `darwin-*` entries that make
  `mise run verify-updater` fail.

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
