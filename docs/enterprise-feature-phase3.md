# Phase 3 — Build (partial)

**Branch**: `feature/multi-file-renderers`

## Delivered
- `install/mod.rs`: `write_agent_files_to` now uses `render::render_files`, writes every file, computes composite hash, records `dests` + `file_hashes`. Backward-compatible via fallback.
- `toolRegistry.ts`: enabled `kimi-agent`, `openclaw-workspace`, `vibe-toml` formats (Kimi, OpenClaw, Vibe now installable).
- Roster tools (Aider, Windsurf) remain gated (need aggregate install flow — deferred).
- Reconcile for multi-file still uses single-render hash; full multi-file reconcile deferred to next iteration (tools not yet exercised in prod reconcile until enabled end-to-end).

## Deferred
- Roster aggregate model + install flow.
- Multi-file reconcile (composite hash comparison on disk).
- remove_agent_files multi-file deletion.
- Parity tests against upstream convert.sh for new formats.

## Evidence
- Code is additive; existing single-file path unchanged when render_files returns one file.
- Build verification blocked by missing Windows SDK linker in this environment; recommend CI gate.
