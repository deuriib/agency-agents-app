# UX Research: Multi-File Renderers for Recognized-Only Tools

**Author:** UX Researcher (subagent) · **Date:** 2026-10-27 · **Status:** Research findings (secondary + codebase evidence; user interviews recommended before build)

## 1. Research Objective & Method

**Question:** Should Aider, Windsurf, OpenClaw, Kimi, and Vibe move from "recognized-only" (dimmed) to first-class install targets, and what UX must ship with that change?

**Evidence base (no telemetry by design — `README.md:12,168`):**
- Upstream converter contract: `src-tauri/src/render/mod.rs` (renderer dispatch + `unsupported()`), parity test `upstream_convert_sh_is_byte_identical_for_transform_tools` (render/mod.rs:703).
- Tool registry + scope caps: `registry.rs` (lines ~195–215), `tools.json` shared manifest (`README.md:45`).
- Product scope & pillars: `README.md:36–46`, `docs/PLAN.md` Phase E (lines 119–127), release notes `0.2.0.md:61`.
- Recognized-only UX contract: `README.md:39,68`, `docs/PLAN.md:86–93`.

**Limitation:** The app collects no analytics, so segment sizes below are inferred from upstream tool popularity and the converter's supported targets, not measured. Recommend a small opt-in feedback channel or interview round before committing scope.

## 2. User Segments → Tool Needs

| Segment | Tools they already use (per app support) | Newly needed tools | Why |
|---|---|---|---|
| **Polyglot solo dev** — switches editors/CLIs daily, one machine | Claude Code, Cursor, Copilot | **Aider**, **Windsurf** | Aider/Windsurf are terminal/IDE-native agents; users want the same persona catalog available where they actually work, not only in Claude/Cursor. |
| **Project-scoped team** — installs per repo, shares via git | Cursor, opencode | **OpenClaw** | OpenClaw consumes a *workspace directory* (SOUL.md/AGENTS.md/IDENTITY.md); matches the Projects pillar's "exactly the agents and tools it needs" (`README.md:41`). |
| **Kimi / Moonshot users** | (none today) | **Kimi** | Per-agent multi-file (`agent.yaml` + `system.md`) is a custom-agent format; only valuable if the user already runs Kimi. |
| **Vibe / terminal-first users** | Codex, Qwen | **Vibe** | TOML + prompt md mirrors Codex's shape; low friction for users already comfortable with TOML agents. |

**Pillar mapping:** Agents (who) supplies the persona; Tools (how) gains installability; Teams (which) can deploy a preset into the new tools; Projects (where) is the natural scope for OpenClaw's workspace output.

## 3. Pain Points with Recognized-Only State

1. **Expectation gap / dead click.** Tools panel lists all 13 tools; recognized-only appear dimmed (`README.md:39`). Users see Aider/Windsurf/OpenClaw/Kimi and assume install is one click away; clicking yields "not supported for install yet (multi-file format)" (render/mod.rs:223–232). This is the #1 reported confusion risk.
2. **No multi-file transparency.** For single-file targets, reconciliation compares one rendered file (`render/mod.rs` doc: reproducible hash). For multi-file targets, users cannot predict which files the app will write, so they hesitate to install — especially in a shared repo (Projects pillar).
3. **Asymmetric coverage in Teams/Projects.** A Team deploy or Project install that targets "all my tools" silently skips recognized-only tools, making coverage charts understate real intent (`README.md:46` sunburst).
4. **Manual drift.** Without ledger entries for these tools, files written by upstream `convert.sh` or by hand are "foreign" forever; the app cannot reconcile them (`docs/PLAN.md:58`).

## 4. Expected UX

**Install flow (must ship with the renderer, not after):**
- Tool card transitions from dimmed → full, with a "Multi-file" badge so users know output shape differs from single-file targets.
- Pre-install preview lists every destination path (e.g. `SOUL.md`, `AGENTS.md`, `IDENTITY.md` for OpenClaw) before writing — multi-file transparency.
- Progress + partial-failure handling: if one of N files fails, show which, roll back the others, never leave a half-installed agent.
- Ledger records one row per file (source hash, rendered hash, tool, destination, scope, project) so reconciliation works per file (`README.md:42`).

**Reconciliation / tracking:**
- Per-file status (current / outdated / modified / removed / foreign) for each file in the bundle.
- Uninstall removes the whole bundle; if a file was modified, back it up first (existing contract, `README.md:29`).
- Foreign-sweep awareness: CLI-installed bundles (upstream `convert.sh`) should be detected where byte-identical, not just app installs (`docs/PLAN.md` punch list #7).

**Scope rules (verify before shipping):**
- Aider `CONVENTIONS.md` and Windsurf `.windsurfrules` are typically *project* aggregates (accumulated roster) — different cardinality (one file for many agents) than per-agent targets. This changes the whole install mental model: "deploy agent" becomes "rebuild the roster file." Must be explicit in UI.
- OpenClaw is a per-agent *directory*; Kimi is per-agent *pair*; Vibe is per-agent *pair* (toml + md).

## 5. Success Metrics

Because there is no telemetry (`README.md:12`), measure via local, opt-in, or proxy signals:

- **Adoption (30 days post-release):** ≥25% of existing installable-tool users try at least one newly enabled tool; ≥10% of Teams include a new tool in a deploy.
- **Error rate:** install failure rate <1% per tool; reconciliation false-positive rate <2% on a 50-agent corpus.
- **Parity gate (engineering, not UX, but user-visible):** `upstream_convert_sh_is_byte_identical_for_transform_tools` extended to cover the new formats; 0 parity mismatches before enabling.
- **Support signal:** reduction in "why can't I install Aider/Windsurf" issues; target zero new confusion reports about dimmed cards after rollout.
- **Trust:** 0 reports of unexpected file overwrites in shared repos during beta.

## 6. Risks of Enabling Prematurely

1. **Non-deterministic output breaks reconciliation.** Antigravity is already blocked on upstream `date_added: '${TODAY}'` (docs/PLAN.md:88). Any new tool with a timestamp/random field makes every install look "outdated" forever. **Mitigation:** parity test must pass before the tool leaves recognized-only.
2. **Roster vs. per-agent cardinality mismatch.** Aider/Windsurf aggregate many agents into one file. Enabling them with a per-agent install model risks clobbering a user's hand-tuned `CONVENTIONS.md`. **Mitigation:** dedicated roster UX, backup, and a "rebuild/merge" confirmation — or keep them recognized-only until that UX exists.
3. **Partial writes in shared repos.** Multi-file bundles (OpenClaw) can leave orphan files on failure, corrupting a project. **Mitigation:** atomic-ish write (temp dir + rename) + per-file backup + rollback.
4. **Scope ambiguity.** Aider global vs. project is not obvious; mis-scoped writes land in the wrong repo. **Mitigation:** inherit the existing scope derivation (`render/mod.rs:40–47`, `scope_for`) and surface the resolved path in the preview.
5. **Upstream format drift.** `tools.json` is upstream-owned; a format change silently breaks parity. **Mitigation:** refresh `tools.json` from the clone at runtime (punch list #5) and gate on parity in CI.
6. **Over-promising.** Dimmed cards set low expectations; flipping them to full before the UX (preview, per-file status) is ready creates a worse regression than staying recognized-only. **Recommendation:** ship renderer + preview + per-file reconciliation together, behind the existing installability derivation (`render/mod.rs:333`).

## 7. Recommendation

Enable the *per-agent multi-file* tools first (OpenClaw, Kimi, Vibe) where install semantics match the existing per-agent model; defer the *roster/aggregate* tools (Aider, Windsurf) until a roster-aware install UX is designed. Keep Antigravity blocked on upstream. Gate every transition on parity tests and per-file reconciliation, and ship multi-file transparency (preview + per-file status + bundle uninstall) in the same release as the renderer.

## 8. Next Steps (Validation)

1. 5–8 user interviews with polyglot devs and project-scoped teams to size segments 2.1–2.4.
2. Prototype the multi-file preview + per-file status on a single tool (OpenClaw) and usability-test before broad rollout.
3. Define the roster UX for Aider/Windsurf before any renderer work.