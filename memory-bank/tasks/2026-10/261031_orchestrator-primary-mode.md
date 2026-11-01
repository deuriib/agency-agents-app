# 261031_orchestrator-primary-mode

## Objective
Resolve issue #22: render `agents-orchestrator` as the primary/main agent for the tools
that distinguish a primary agent from subagents, without changing specialist output.

## Outcome
- ✅ opencode: orchestrator renders `mode: primary`; specialists keep `mode: subagent`.
- ✅ Antigravity: orchestrator renders `mainAgent: true` (replacing `subagent: true`);
  specialists keep `subagent: true` byte-identically.
- ✅ Render policy mirrors upstream `PRIMARY_AGENT_SLUGS` (slugified frontmatter name).
- ✅ 348 backend tests pass (1 ignored); render suite 34 pass.
- ✅ Independent EvidenceQA PASS; code review APPROVE WITH NITS; reality-checker
  confirmed the code is ship-quality.

## Files Modified
- `src-tauri/src/render/mod.rs` — primary-agent policy, per-tool role flags, leak guard,
  parity-gate normalization, tests.
- `memory-bank/decisions.md` — ADR for the deliberate `convert.sh` divergence.
- `docs/release-notes/unreleased.md` — known release note.
- `memory-bank/tasks/2026-10/README.md` — achievement entry.

## Patterns Applied
- `memory-bank/contracts.md` §B/§E — deterministic render + `rendered_hash` reconciliation.
- Upstream converter parity: `agency_agents/core/converter.py#PRIMARY_AGENT_SLUGS`.

## Integration Points
- `render::render` is called by `install::*` and `agent_diff`; only the orchestrator's
  bytes change, so reconcile flags at most two ledger rows.
- `render::render_files` now forwards the agent to `render` (the old `_agent` would have
  made the policy a no-op for the multi-file wrapper).

## Open Follow-ups
- #26 — port upstream's per-role Antigravity `tools:` list (`MAIN_AGENT_TOOLS` /
  `SUBAGENT_TOOLS`).
- #27 — confirm Antigravity's runtime behavior when `mainAgent` is omitted for
  specialists (upstream omits it; the binary's `yaml:"mainAgent,omitempty"` implies
  `false`, unverified at runtime).
