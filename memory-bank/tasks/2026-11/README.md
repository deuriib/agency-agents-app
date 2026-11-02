# November 2026 — Antigravity parity + updater pipeline

## Current State
Post-v0.6.0. The renderer mirrors upstream's Antigravity role policy *and* its per-role
tool declarations. PR #38 is merged (`a453bd2`): the in-app updater is installable
end-to-end and stable/beta channels are live in code. `release-please` opened PR #39
(`0.8.0`), currently blocked on the signing secrets.

## Goals
- [ ] Confirm Antigravity runtime semantics for the role flags (issue #27).
- [ ] Set the updater signing secrets, merge PR #39, and publish/verify 0.8.0.

## Recent Achievements
- Fixed the in-app updater and closed #23/#24 (PR #38): the release pipeline now
  publishes signed `latest.json` manifests, Linux/Windows updater artifacts are signed,
  the Linux tarball is regenerated after the Wayland repack, stable/beta channels are
  persisted and selectable, and the auto-install consent keys exist in all 11
  translations. Added `mise run verify-updater` and `mise run set-updater-secrets`.
  See [261102_updater-pipeline.md](./261102_updater-pipeline.md).
- Added a Draft 2020-12 JSON Schema for the tool registry (issue #30): `src-tauri/data/_schema.json`
  validates the bundled `tools.json` in the IDE (VS Code mapping) and in `cargo test --test tools_schema`,
  which also asserts the schema still rejects malformed entries. See
  [261101_tools-json-schema.md](./261101_tools-json-schema.md).
- Verified Antigravity role-flag defaults at runtime (issue #27): no change needed; omitted
  `mainAgent` defaults to `false` (protobuf `omitempty`), unknown keys are tolerated, the CLI
  ignores flags for `--agent` selection. Found a stale foreign artifact (`mainAgents: true`)
  that will be replaced by reconcile. See
  [261101_antigravity-role-flag-runtime.md](./261101_antigravity-role-flag-runtime.md).
- Declared per-role Antigravity tool lists (issue #26): `agents-orchestrator` gets the 11-tool
  orchestration surface, specialists get the 7 execution tools. See
  [261101_antigravity-tool-lists.md](./261101_antigravity-tool-lists.md).
