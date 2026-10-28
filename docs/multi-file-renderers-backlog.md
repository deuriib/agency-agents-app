# Multi-File Renderer Backlog — RICE Scored

> Companion to `docs/multi-file-renderers-architecture.md`. Scope: ship native
> multi-file renderers for Aider, Windsurf, Kimi, OpenClaw, and Vibe so the five
> "recognized-only" tools become first-class install targets. Antigravity already
> ships via `gemini-md` and is the precedent.

## Assumptions (state explicitly)

- **User base**: ~10,000 active app users (no telemetry; estimate from release
  download counts). Reach figures below are estimated monthly-active users per
  tool, derived from upstream `agency-agents` adoption signals and tool market
  share. Treat as ordinal, not precise.
- **Sprint**: 2 weeks, team capacity ~6 person-weeks per sprint (2 engineers).
- **Confidence** reflects format verification status against upstream
  `convert.sh` / `scripts/install.sh`, not implementation confidence.
- Tools already declared in `src-tauri/data/tools.json` with dest templates; the
  gap is purely in `render/mod.rs` (no `format` arm) and the install/reconcile
  core.

## Sub-features

| # | Sub-feature | Tools | MoSCoW |
|---|-------------|-------|--------|
| 1 | Roster tools combined-file render | Aider, Windsurf | Must |
| 2 | Kimi per-agent multi-file render | Kimi | Must |
| 3 | OpenClaw per-agent multi-file render | OpenClaw | Should |
| 4 | Vibe toml + prompt md render | Vibe | Must |
| 5 | UI enablement + tool status | All five | Should |
| 6 | Tests / parity | All | Must |
| 7 | Docs | All | Must |

## RICE scoring

Formula: `RICE = (Reach × Impact × Confidence) / Effort`. Impact on 0.25–3 scale
(3 = massive). Confidence 0–1. Effort in person-weeks.

| # | Sub-feature | Reach (users/mo) | Impact | Confidence | Effort (pw) | RICE |
|---|-------------|------------------|--------|------------|-------------|------|
| 5 | UI enablement + tool status | 10,000 | 1.0 | 0.90 | 1.0 | **9,000** |
| 1 | Roster tools (aider, windsurf) | 2,500 | 2.0 | 0.70 | 2.5 | **1,400** |
| 7 | Docs | 1,000 | 0.5 | 0.95 | 0.5 | **950** |
| 2 | Kimi per-agent multi-file | 800 | 2.0 | 0.80 | 1.5 | **853** |
| 6 | Tests / parity | 1,000 | 1.0 | 0.95 | 2.0 | **475** |
| 3 | OpenClaw per-agent multi-file | 500 | 2.0 | 0.75 | 1.5 | **500** |
| 4 | Vibe toml + md | 400 | 1.5 | 0.80 | 1.2 | **400** |

Notes on the ordering:

- **UI enablement (#5) scores highest** because it is cheap (1 pw) and touches
  every user who sees dimmed tools, but it is gated on at least one renderer
  shipping — otherwise enabling only re-exposes the existing "unsupported" error.
  Treat as a Should that lands in Sprint 2 once renderers exist.
- **Roster tools (#1) carry the highest *product* value** among renderers: two
  tools, one accumulated file, and they unblock the largest single user segment
  (Aider + Windsurf). They also carry the most engineering risk (aggregate
  mutex, re-render on install/uninstall), hence the lower confidence (0.70).
- **Kimi (#2) before OpenClaw (#3)**: higher reach and a simpler two-file
  shape; OpenClaw's three-file shape reuses the same code path.
- **Tests (#6) and Docs (#7) are Must** despite modest RICE: they are gates,
  not features. Docs RICE is inflated by low effort; tests RICE is low because
  reach is internal, but TST-01/TST-03 make them non-negotiable.

## MoSCoW classification

- **Must have (Sprint 1–2)**: #1 roster tools, #2 Kimi, #4 Vibe, #6 tests/parity,
  #7 docs, plus Phase 0–2 of the architecture doc (schema, render API, install
  core).
- **Should have (Sprint 2)**: #5 UI enablement, #3 OpenClaw.
- **Could have (post-1.0)**: per-file `agent_diff` UI view; roster managed-block
  markers; GitHub-backed app features.
- **Won't have (this initiative)**: Ollama/LM Studio system-prompt target,
  automatic install toggle, new tool additions beyond the five, cross-tool
  team deploy.

## Recommended 2-sprint plan

Capacity: 6 pw/sprint × 2 = 12 pw total. Planned spend: ~11.2 pw (0.8 pw buffer).

### Sprint 1 (6 pw) — foundation + first renderers

| Week | Work | Owner | Pw |
|------|------|-------|----|
| 1 | Phase 0 schema (types.rs `dests`/`file_hashes`/`aggregate_slugs`, load_ledger backfill) | backend | 0.5 |
| 1 | Phase 1 render API (`RenderedFile`, `render_files`, shim) + registry format variants | backend | 1.0 |
| 1–2 | Phase 2 install core rewrite (`write_agent_files_to` pairing + transactional rollback) | backend | 1.5 |
| 2 | Kimi renderer + dests (#2) | backend | 1.0 |
| 2 | Vibe renderer + dests (#4) | backend | 0.8 |
| 2 | Unit tests for render_files + composite hash (#6) | backend | 1.2 |

Sprint 1 exit: Kimi and Vibe installable end-to-end with reconcile; roster
tools still dimmed but no longer crash.

### Sprint 2 (6 pw) — roster + enablement + closeout

| Week | Work | Owner | Pw |
|------|------|-------|----|
| 3 | Roster renderer + aggregate mutex + install/uninstall re-render (#1) | backend | 2.0 |
| 3 | OpenClaw renderer (#3) | backend | 0.8 |
| 3 | Reconcile edits: per-file composite, roster groups, foreign sweep leaves (#6) | backend | 1.2 |
| 4 | UI enablement: flip `installable` for the five tools, tool status badges (#5) | frontend | 1.0 |
| 4 | Parity tests (ignored, `AGENCY_AGENTS_PARITY_ROOT`) for all five (#6) | backend | 0.6 |
| 4 | Docs: README supported-targets table, contracts.md §B matrix, systemPatterns (#7) | docs | 0.4 |

Sprint 2 exit: all five tools first-class; parity tests green; docs updated in
same PR (DOC-01).

## Scope guardrails (against scope creep)

1. **No new tools.** Only the five declared in `tools.json` (aider, windsurf,
   kimi, openclaw, vibe). Antigravity already ships. Do not add Ollama/LM Studio
   or any other target in this initiative.
2. **No roster marker blocks.** Roster files are fully re-rendered aggregates;
   do not introduce managed-block parsing or partial edits.
3. **No per-file ledger rows.** Keep one `InstallRecord` per logical install;
   use composite hash + `file_hashes` map. Do not split into per-file rows.
4. **No automatic install toggle / GitHub-backed features.** These are separate
   backlog items with their own RICE; do not bundle.
5. **No UI redesign.** Only flip installability and status badges; reuse
   existing Tools panel components.
6. **No `render()` signature break.** Keep the shim; migrate call sites only
   after all five ship (single PR, post-initiative).
7. **No schema migration command.** Load-time `#[serde(default)]` backfill only;
   zero user action.
8. **Docs in same PR.** README supported-targets table + contracts.md §B +
   systemPatterns update ship with the code, not "later" (DOC-01).
9. **Parity tests gated.** Ignored by default; run only with
   `AGENCY_AGENTS_PARITY_ROOT`. Do not block CI on upstream repo availability.
10. **Stop condition.** If roster aggregate mutex or transactional rollback
    cannot land in Sprint 2, ship Kimi + Vibe + UI enablement for those two and
    defer roster tools to Sprint 3 rather than shipping a half-transactional
    write path.

## Risks

- **Roster re-render cost O(n)** per install — acceptable for user-driven
  installs; add per-`(tool, project)` async mutex to prevent interleaving.
- **Composite hash framing** must be byte-stable; length-prefix to avoid
  collisions (see architecture doc §2).
- **Migration of old ledgers**: `dests` backfilled from `dest`; old single-file
  rows remain valid. Reconcile falls back to `dests[0]` when `file_hashes`
  empty.
- **Upstream format drift**: confidence 0.70–0.80; verify against
  `convert.sh` before each renderer lands (parity test).

## Success metrics

- All five tools move from "recognized-only" to installable in Tools panel.
- Parity tests green for all five against upstream `convert.sh`.
- Reconcile correctly classifies composite multi-file installs as
  Current/Outdated/Modified/Removed.
- Zero half-written multi-file sets (transactional write verified by test).
- README supported-targets table updated; contracts.md §B matrix updated.

## References

- `docs/multi-file-renderers-architecture.md` (decisions 1–5)
- `src-tauri/src/render/mod.rs` (`render`, `dests`, `render_with_hash`)
- `src-tauri/src/install/mod.rs` (`write_agent_files_to`, `installs_reconcile`,
  `bytes_match_render`, `agent_units`)
- `src-tauri/src/types.rs` (`InstallRecord`, `InstalledAgent`)
- `src-tauri/data/tools.json` (dest templates, format keys)
- `memory-bank/contracts.md` §B (install-target matrix), §E (determinism)
