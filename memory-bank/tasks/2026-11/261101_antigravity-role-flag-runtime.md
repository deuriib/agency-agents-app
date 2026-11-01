# 261101_antigravity-role-flag-runtime

## Objective
Resolve issue #27: establish what Antigravity does with its agent role flags, decide whether
specialists must emit an explicit `mainAgent: false`, and confirm the provenance of the stale
`mainAgents` artifact found on this machine.

## Outcome
No renderer change. The renderer keeps emitting exactly one role flag (`mainAgent: true` for
the orchestrator, `subagent: true` for specialists), matching upstream bytes.

## Findings (all commands executed locally, Antigravity CLI 1.4.12)

1. **Agent discovery is a real YAML parse.** An agent file with malformed frontmatter
   (`description: [unclosed`) is silently dropped: with five valid probe agents present,
   `antigravity agent` returned 152 entries (147 installed + 5 probes) and the malformed
   probe was absent.
2. **Unknown keys are tolerated, not fatal.** The same discovery lists
   `agents-orchestrator`, whose on-disk frontmatter carries `mainAgents: true` (plural).
   The binary only knows `mainAgent` (`yaml:"mainAgent,omitempty"`) and the changelog lists
   `mainAgent`, `subagent`, `hidden`, `inheritMcp`, `commandExecutionPolicy` — so the plural
   key is inert: the agent stays discoverable but is never promoted.
3. **`hidden: true` does not filter `antigravity agent`.** A probe with `hidden: true` still
   appears in the list, so that subcommand reports discovery, not the picker.
4. **`--agent` ignores the role flags.** `antigravity --agent zz-probe-plural -p hi` (the
   probe carried `subagent: true`) got past agent resolution and failed only at the model
   call (`RESOURCE_EXHAUSTED`). A `subagent: true` agent is therefore selectable as the
   session agent; the role flags govern the picker/GUI surface, not CLI selection.
5. **An omitted `mainAgent` is `false`.** The binary's agent struct serializes the field as
   `yaml:"mainAgent,omitempty"`, a bool whose zero value is false; the changelog (1.1.6)
   confirms the field exists alongside `subagent`. Omission is therefore not a promotion.
6. **Live end-to-end verification is blocked on this machine.** The CLI reports
   `error getting token source: You are not logged into Antigravity` and the model call
   fails with `Individual quota reached ... Resets in 115h34m46s`. Picker behaviour
   (which agent the GUI offers as main) could not be observed, only inferred.
7. **The stale artifact is foreign, and drift is provable.** The install ledger records
   `renderedHash 73dc5fa3…` for
   `~/.gemini/config/agents/agents-orchestrator.md`, while the file on disk hashes to
   `6778aa71…`. Its body (`subagent: true` + `mainAgents: true` + block-style `tools:`)
   was never produced by this repo: `git log --all -S mainAgents` returns zero commits in
   both this repo and the upstream clone. Reconcile will flag it `Outdated` on the hash
   mismatch, and a reinstall replaces it with the app's shape.

## Decision
- Keep one role flag per agent; do **not** emit `mainAgent: false` for specialists.
  Rationale: it matches upstream bytes, the omitted bool already reads as false, and the
  CLI `--agent` path ignores the flags entirely — an explicit false would add divergence
  without changing behaviour.
- Treat "primary agent" as a picker/GUI concept. Document it as such in the renderer comment
  when the next Antigravity change touches that arm.
- The plural-key typo is not something the app can detect; reconcile is hash-based, so the
  fix is reinstall.

## Files Modified
- `memory-bank/decisions.md` — ADR with the findings and the decision.
- `memory-bank/tasks/2026-11/README.md` — achievement entry.
- No source change.

## Open Questions
- Picker behaviour with an authenticated session: does a `subagent: true` agent appear in
  the main-agent picker? Re-check when quota resets / the CLI is logged in. This is the only
  remaining unknown, and it does not change the rendered bytes either way.
