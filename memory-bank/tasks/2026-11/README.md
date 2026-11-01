# November 2026 — Antigravity parity

## Current State
Post-v0.6.0. The renderer now mirrors upstream's Antigravity role policy *and* its per-role
tool declarations.

## Goals
- [ ] Confirm Antigravity runtime semantics for the role flags (issue #27).

## Recent Achievements
- Verified Antigravity role-flag defaults at runtime (issue #27): no change needed; omitted
  `mainAgent` defaults to `false` (protobuf `omitempty`), unknown keys are tolerated, the CLI
  ignores flags for `--agent` selection. Found a stale foreign artifact (`mainAgents: true`)
  that will be replaced by reconcile. See
  [261101_antigravity-role-flag-runtime.md](./261101_antigravity-role-flag-runtime.md).
- Declared per-role Antigravity tool lists (issue #26): `agents-orchestrator` gets the 11-tool
  orchestration surface, specialists get the 7 execution tools. See
  [261101_antigravity-tool-lists.md](./261101_antigravity-tool-lists.md).
