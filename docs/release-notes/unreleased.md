## Unreleased

> Staging file. Rename to `docs/release-notes/<version>.md` when the next version is cut.

### Highlights

### Quality Gates

### Known Release Notes

- **Antigravity tool declarations (issue #26)**: every Antigravity agent now declares its
  tool surface in frontmatter — the orchestrator gets the 11-tool orchestration list
  (including `invoke_subagent`), specialists get the 7 execution tools. Source
  Claude-style tool names are replaced. All Antigravity installs will show as Outdated once
  and need an update.
- **Primary agent support (issue #22)**: `agents-orchestrator` now installs as the
  primary/main agent where the tool supports it — `mode: primary` for opencode and
  `mainAgent: true` for Antigravity. Specialists keep `mode: subagent` / `subagent: true`.
  Existing installs of the orchestrator for those two tools will show as Outdated and need
  an update; other agents are unchanged.
