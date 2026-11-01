# 261101_antigravity-tool-lists

## Objective
Resolve issue #26: port upstream's per-role Antigravity `tools:` list into the renderer so
every rendered agent declares its tool surface instead of inheriting the harness default.

## Outcome
- ✅ Primary/main agent (`agents-orchestrator`) renders `mainAgent: true` + the 11-tool
  orchestration list (`invoke_subagent`, `manage_subagents`, `send_message`, `manage_task`,
  plus the seven execution tools).
- ✅ Every specialist renders `subagent: true` + the 7 execution tools, never a delegation tool.
- ✅ The emitted list replaces any source `tools:` frontmatter — Claude-style names
  (`Read`, `Write`, `Bash`, …) never reach Antigravity output.
- ✅ `cargo test --lib`: 350 passed, 1 ignored (pre-existing parity gate).
- ✅ Independent EvidenceQA PASS: upstream converter run for real (290 agents) matched the
  app's `tools:` and role-flag lines on all 274 agents present in both — 0 divergence.

## Files Modified
- `src-tauri/src/render/mod.rs` — `MAIN_AGENT_TOOLS` / `SUBAGENT_TOOLS` consts, `flow_list`
  helper, `antigravity-md` arm emits the role's tool line, five tests updated or added.
- `docs/release-notes/unreleased.md` — known release note for the mass Antigravity update.
- `memory-bank/decisions.md` — ADR for the port.
- `memory-bank/tasks/2026-11/README.md`, `memory-bank/toc.md` — tracking.

## Patterns Applied
- `memory-bank/contracts.md` §B/§E — deterministic render + `rendered_hash` reconciliation.
- Upstream parity: `agency_agents/core/converter.py#AntigravityRenderer`
  (`MAIN_AGENT_TOOLS`, `SUBAGENT_TOOLS`, `frontmatter_overrides`).

## Integration Points
- `render::render` → `install::*` / `agent_diff`: every Antigravity render changes bytes, so
  the whole Antigravity scope flips to `Outdated` once on the next check. Expected, one time.
- The `convert.sh` parity gate is unaffected: it does not cover `antigravity-md`.

## Notes
- Shape stays unquoted (`tools: [a, b, c]`) to match upstream's serializer; `name` and
  `description` keep the app's YAML-safe single-quote style.
- Leak-guard tests inspect the frontmatter head only, so a persona that mentions a
  delegation tool in prose cannot fail the suite spuriously.
- The app still does not emit upstream's `version` / `vibe` / `color` frontmatter for
  Antigravity; that divergence predates this change and is out of scope.

## Open Follow-ups
- #27 — confirm Antigravity's runtime behavior when `mainAgent` is omitted for specialists.
