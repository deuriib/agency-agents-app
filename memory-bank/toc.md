# Memory Bank — Table of Contents

- `projectbrief.md` — vision, thesis, what it is/isn't.
- `systemPatterns.md` — corpus-copy model, 2 indexes, deterministic renderer, 5-state
  reconciliation, both scopes, domain module map, frontend sections.
- `contracts.md` — **the build spec**: core types, install-target matrix, command surface,
  frontend stores, determinism rules. Build against this.
- `decisions.md` — ADRs (fork, Plan B, corpus-copy, state tracking, hash-match provenance,
  both scopes, vulns→Quality/services→Tools/Snapshots→Loadouts, native rule compilation
  on the shared ledger).
- `phases/phase-roadmap.md` — Phase 0–6 plan + parallelization map.
- `activeContext.md` — current state, reference clones, temp scaffolding, immediate next.
- `agentLog.md` — append-only operational log of the team/loop.
- `NEXT-SESSION.md` — **read first after a compaction**: resume notes, the v2 plan, open decisions,
  known issues, and hard-won gotchas (each_key_duplicate, icon re-embed, port 1430, screenshots).
- `tasks/2026-06/` — monthly README + per-task docs (Phase 0 fork, Phase 1 corpus+discover, etc.).
- [tasks/2026-09/README.md](tasks/2026-09/README.md) — September 2026 (AppImage child-env fix; the release backlog)
- [tasks/2026-11/README.md](tasks/2026-11/README.md) — November 2026 (Antigravity per-role tool declarations; tools.json schema gate; updater pipeline; Rules integration)
  - [261103_rules-integration.md](tasks/2026-11/261103_rules-integration.md) — native rule compilation, second corpus, ledger domain discriminator, Rules workspace
  - [261102_updater-pipeline.md](tasks/2026-11/261102_updater-pipeline.md) — signed `latest.json`, stable/beta channels, and the release runbook
