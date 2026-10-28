## Active Context — Agency Agents

**State (2026-10-28)**: **v0.4.3 is shipped.** `main` is current. 
The app has transitioned from a single-file agent model to a multi-file renderer architecture.

**Landed in v0.4.3**:
- **Multi-file renderers**: First-class support for Kimi, OpenClaw, and Mistral Vibe.
- **Multi-file Install Ledger**: Ledger now tracks `dests` and per-file hashes with composite hash validation.
- **Enterprise-Ready**: Phases 1-4 of Enterprise Feature development completed (UX research, architecture, and backlog).
- **Infrastructure**: Release workflow automated on push to main.

**Current Focus (Post-v0.4.3)**:
1. **Multi-file Reconcile/Removal**: Implement logic to detect drift and uninstall agents that occupy multiple files.
2. **Roster Aggregation**: Enable installation flows for Aider and Windsurf.
3. **Parity Testing**: Ensure multi-file output matches upstream expectations across all renderers.

**Known Constraints**:
- Multi-file logic must remain backward-compatible with single-file ledgers.
- Install flow for complex rosters (Aider/Windsurf) requires aggregate install handling.

**Gates**:
- `npm run check` (svelte-check) must be 0 errors.
- `cargo test` for backend validation.

---
## History (Obsolete)
v0.3.1 and prior contexts are now deprecated. Refer to `docs/release-notes/` for history.

