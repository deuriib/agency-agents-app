## Active Context — Agency Agents

**State (2026-10-28)**: **v0.6.0 is cut.** The release PR bumps all five manifests; merging to `main` makes `release.yml` create tag `v0.6.0` and a draft release.

- **Landed in v0.6.0**:
  - **Opt-in automatic installs**: the Settings → Updates toggle now drives a real background download → verify → install flow. Off by default, consent on first enable, Offline Mode a hard gate, relaunch always user-initiated.
  - **Scheduler-owned**: the Rust scheduler wakes ~60s after launch (jittered), then every 24h; failures back off 1h/6h/24h; an inactive app polls every 5min. No settings migration.
  - **Single-flight installs**: one atomic slot shared by the automatic and manual paths (`install_in_progress` when busy).
  - **Fixes**: single-file renders now hash the file bytes (reconcile no longer reports `Modified` for every single-file install); `antigravity-md` registered in `IMPLEMENTED_FORMATS`; renderer tests use the current tool ids; eleven stale `autoInstallHint` strings corrected.
- **Landed in v0.5.1**: version bump plus the manifest-drift repair (all five manifests in lockstep). Its release notes claimed `antigravity-md`, which actually landed in v0.6.0 — see `decisions.md`.
- **Landed in v0.5.0**: version bump and standardized release notes.
**Landed in v0.4.3**:
- **Multi-file renderers**: First-class support for Kimi, OpenClaw, and Mistral Vibe.
- **Multi-file Install Ledger**: Ledger now tracks `dests` and per-file hashes with composite hash validation.
- **Enterprise-Ready**: Phases 1-4 of Enterprise Feature development completed (UX research, architecture, and backlog).
- **Infrastructure**: Release workflow automated on push to main.
**Current Focus**:
1. **Auto-install smoke test**: exercise the full path on a signed build (outstanding release condition).
2. **i18n native pass**: review the eleven corrected `autoInstallHint` strings.
3. **Multi-file Reconcile/Removal**: detect drift and uninstall agents that occupy multiple files.
4. **Roster Aggregation**: enable installation flows for Aider and Windsurf.
5. **Parity Testing**: ensure multi-file output matches upstream expectations across all renderers.

**Known Constraints**:
- Multi-file logic must remain backward-compatible with single-file ledgers.
- Install flow for complex rosters (Aider/Windsurf) requires aggregate install handling.

**Gates**:
- `npm run check` (svelte-check) must be 0 errors.
- `cargo test` for backend validation.

---
## History (Obsolete)
v0.3.1 and prior contexts are now deprecated. Refer to `docs/release-notes/` for history.

