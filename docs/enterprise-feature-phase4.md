# Phase 4 — Hardening (evidence collected)

**Branch**: `feature/multi-file-renderers`

## Evidence
- Added unit tests in `render/mod.rs`: `render_files_kimi_is_multi`, `render_files_vibe_is_multi`, `render_files_openclaw_is_multi`, `render_files_single_wraps_one`.
- Compliance scan P0 gates addressed in implementation: dests rooted at home/project (unchanged), slug allowlist (unchanged), no shell-out (renderers pure), allowlisted frontmatter only.
- MIT attribution retained (no new license files).

## Deferred / needs CI
- Parity test vs upstream `convert.sh` for kimi/openclaw/vibe (requires upstream repo + `AGENCY_AGENTS_PARITY_ROOT`).
- Multi-file reconcile + removal tests.
- Roster (Aider/Windsurf) remains gated.
- Performance: no new hot paths; render_files is O(n) over files.

## Reality Checker verdict
NEEDS WORK (expected default): multi-file reconcile and removal not yet implemented; roster gated. Acceptable for incremental merge of the foundation + first three tools once reconcile lands.

Proceed to Phase 5 (Rollout) only after reconcile + parity tests green in CI.
