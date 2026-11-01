## Unreleased

> Staging file. Rename to `docs/release-notes/<version>.md` when the next version is cut.

### Highlights

### Quality Gates

### Known Release Notes

- **Primary agent support (issue #22)**: `agents-orchestrator` now installs as the
  primary/main agent where the tool supports it — `mode: primary` for opencode and
  `mainAgent: true` for Antigravity. Specialists keep `mode: subagent` / `subagent: true`.
  Existing installs of the orchestrator for those two tools will show as Outdated and need
  an update; other agents are unchanged.
