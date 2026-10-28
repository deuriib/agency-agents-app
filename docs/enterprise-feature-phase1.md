# Phase 1 — Requirements & Architecture: Multi-File Renderers

**Runbook**: Enterprise Feature Development (NEXUS-Sprint) | **Feature**: Native multi-file renderers for Aider, Windsurf, Kimi, OpenClaw, Vibe
**Date**: 2026-10-27 | **Status**: Phase 1 complete — awaiting Architecture Review gate

## 1. Feature scope (single sentence)

Enable first-class installability for five currently recognized-only tools (Aider, Windsurf, Kimi, OpenClaw, Vibe) by implementing deterministic multi-file renderers, without breaking the existing single-file `render()` contract or reconciliation.

## 2. Stakeholder alignment (Project Shepherd)

| Stakeholder | Interest | Cadence | Owner |
|---|---|---|---|
| End users (polyglot devs, teams) | Install agents into more tools | Weekly status | Project Shepherd |
| Maintainer (Michael) | Parity + release quality | Bi-weekly | Executive Summary Generator |
| Compliance team | File-write safety, no shell-out, MIT attribution | Monthly | Legal Compliance Checker |
| Finance | Budget (2 sprints, ~11.2 pw) | Monthly | Finance Tracker |
| Engineering | Backward-compat render contract | Daily | Agents Orchestrator |

## 3. User research (UX Researcher)

See `docs/ux-research-multi-file-renderers.md`. Key findings: recognized-only state causes dead clicks and asymmetric Teams/Projects coverage; users need pre-install file preview and bundle uninstall/rollback. Staged recommendation: roster tools first (highest reach), then Kimi, then OpenClaw, Vibe last.

## 4. Compliance requirements (Legal Compliance Checker)

See `memory-bank/scans/multi-file-renderers-compliance.md`. P0 gates: dests rooted at home/project; slug allowlist; no shell-out; renderers pure functions; allowlisted frontmatter only; no PII/secrets; `tools.json` parity via `check-tools.sh`. Draft pending human review (DENY-07).

## 5. UX foundation (UX Architect / UI Designer)

- Tools panel: currently dimmed tools become enabled once their `format` is implemented; add per-tool file-count badge ("installs N files").
- Install modal: multi-file preview lists every file to be written before confirm; "bundle" uninstall removes all files atomically.
- Reconciliation: per-file state surfaced via existing 5-state model; composite hash is the reconcile key.
- Brand: no new brand assets needed; reuse existing tool icons.

## 6. System architecture (Backend Architect)

See `docs/multi-file-renderers-architecture.md`. Recommended design:
- Add `render_files() -> Vec<RenderedFile>`; keep `render()` as shim.
- Composite SHA-256 hash + per-file `file_hashes` map on `InstallRecord` (serde-defaulted).
- Roster tools as aggregate containers keyed by `(tool, project)`.
- Partial-write safety via temp+rename + rollback.
- Backward-compatible ledger migration at load time.

## 7. RICE backlog (Sprint Prioritizer)

See `docs/multi-file-renderers-backlog.md`. 7 sub-features, 2-sprint plan (11.2 pw of 12 pw). MoSCoW: Must = roster, Kimi, Vibe, tests, docs; Should = UI enablement, OpenClaw.

## 8. Brand impact (Brand Guardian)

No new brand surfaces. Tool icons already present in `assets/tools/` for aider/windsurf/kimi/openclaw/vibe (verify). Ensure dimmed→enabled transition uses existing accent colors and AA contrast. No brand voice changes.

## 9. Budget framework (Finance Tracker)

- 2 sprints × 6 pw = 12 pw capacity; plan uses 11.2 pw (6.7% buffer).
- No new infrastructure cost; tests run in existing CI.
- Risk: roster mutex complexity may overrun; stop condition defined in backlog.

## 10. Architecture Review Gate (Project Shepherd + Reality Checker)

| Criterion | Status |
|---|---|
| Reuse over creation (extend render/mod.rs, not rewrite) | ✅ Extends existing render dispatch |
| No new files without justification | ✅ New docs only; code extends existing modules |
| Backward compat (render() shim, serde defaults) | ✅ |
| Compliance P0 gates addressed | ✅ (pending human review) |
| Test plan defined | ✅ (parity test + unit tests) |
| Budget within limits | ✅ |

**Verdict**: ✅ PASS — proceed to Phase 2 (Foundation) pending user approval.

## Next actions (Phase 2)
1. DevOps: feature branch + CI gate (cargo fmt --check, parity test).
2. Backend: implement `render_files()` + `RenderedFile`, ledger migration, roster aggregate model.
3. Frontend: enable tools in registry-derived installability (no registry change until renderer lands).
4. Evidence Collector: baseline screenshots of dimmed state.

Awaiting approval to enter Phase 2.
