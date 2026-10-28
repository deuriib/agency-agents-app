# Compliance Requirements Scan — Native Multi-File Renderers
## Aider (roster) · Windsurf (roster) · Kimi (per-agent multi-file) · OpenClaw (per-agent multi-file) · Vibe (toml+md) · Antigravity (gemini-md, already live)

**Scope**: feature introducing native multi-file renderers for five currently recognized-only tools. This scan gates implementation; no code lands until each item is addressed or explicitly waived.

**Severity legend**: **P0** = blocks merge / regulatory or safety violation; **P1** = must fix before release; **P2** = should fix, may ship with waiver; **P3** = improvement / hygiene.

---

## 1. File-write safety / path traversal — P0

- **[P0]** All dests must be rooted at `home` (user scope) or `project_root` (project scope) exactly as `render::dests` does; never accept a caller-supplied absolute path. Dest templates come only from `src-tauri/data/tools.json` (registry), never from user input. Cite: `render/mod.rs::dests`, `install/mod.rs::do_install` (via `candidate_dests`).
- **[P0]** Slug validation: renderers derive `{slug}` from `output_slug()` → `slugify(name)`; the file-name token must contain only `[a-z0-9-]`. Reject empty slugs, slugs of `.`, `..`, or any value containing `/`, `\`, `:`, null, or path separators. Existing `slugify` already lowercases + dashes; add an explicit allowlist assertion at the write chokepoint. Cite: `render/mod.rs::slugify`, `output_slug`.
- **[P0]** Multi-file dests (Kimi `agent.yaml`+`system.md`, OpenClaw `SOUL.md`/`AGENTS.md`/`IDENTITY.md`, Vibe `*.toml`+`prompts/*.md`) must create only the directories implied by the registry template; no `mkdir -p` on arbitrary user paths. Parent creation must be explicit and bounded to the chosen root. Cite: `install/mod.rs::write_agent_files_to`, `util/fs.rs::atomic_write` (parent must pre-exist by contract).
- **[P1]** Roster tools (Aider `CONVENTIONS.md`, Windsurf `.windsurfrules`) write a single accumulated file: ensure install/uninstall operates on the registered dest only and never follows symlinks out of the root (use `symlink_metadata` + refuse symlink dests). Cite: `install/mod.rs::remove_agent_files`, `remove_file_strict`.
- **[P1]** Prevent dest collisions: two agents must never resolve to the same path; roster accumulation must be idempotent and byte-deterministic. Cite: `render/mod.rs::sha256_hex` (reproducibility invariant).
- **[P2]** Cap total written bytes via existing `MAX_INSTALLED_BYTES` / `read_capped` for any read-back; apply the same cap to render output before write. Cite: `util/fs.rs::read_capped`, `install/mod.rs::MAX_INSTALLED_BYTES`.

## 2. No shell-out / no arbitrary command execution — P0

- **[P0]** Renderers must be pure functions of `(Agent, raw_source)` with no `Command`/`process`/`shell` invocation. Multi-file renderers must follow the same contract (no templating via external binaries). Cite: `render/mod.rs` module doc ("Every renderer is a PURE function").
- **[P0]** Tool detection (`detect()`) and version probing (`probe_version`) already use `std::process::Command` only for the tool's own binary with sanitized env via `util/proc.rs`. New tools must not expand this to arbitrary paths; version binary comes only from `tools.json::version.bin`. Cite: `install/mod.rs::detect`, `probe_version`, `version_cmd`, `util/proc.rs::sanitize`.
- **[P1]** Roster installs must not invoke the target CLI to "register" agents; file write only. Cite: `install/mod.rs::do_install`.
- **[P2]** CI must run `cargo audit` / dependency scan; no new crate with `unsafe` shell helpers. Cite: `.github/workflows` (verify present).

## 3. Data handling (no PII, no secrets in rendered files) — P0

- **[P0]** Rendered content derives only from corpus frontmatter (`name`, `description`, `body`, `color`, `tools`, `vibe`) plus deterministic transform. No user PII, no env vars, no host paths, no tokens may be interpolated. Cite: `render/mod.rs::source_field`, `source_body`.
- **[P0]** Corpus agents are static markdown; renderers must not echo raw user input. Any new field added for multi-file shapes (e.g. OpenClaw `IDENTITY.md`) must be allowlisted and escaped per format (YAML/TOML/Markdown). Cite: `yaml_quote`, `toml_escape`.
- **[P1]** Backups (`backups/`) and ledger (`installed.json`) store rendered bytes + hashes only, never source secrets. Existing `atomic_write` + `read_capped` chokepoints cover this. Cite: `util/fs.rs::atomic_write`, `read_capped`.
- **[P1]** No secrets in logs/toasts/error messages: errors must not include file contents or rendered body. Cite: `error.rs` (verify `AppError::Io` messages don't leak paths beyond display).

## 4. Supply chain (tools.json upstream parity, check-tools.sh) — P0

- **[P0]** New tool entries (`aider`, `windsurf`, `kimi`, `openclaw`, `vibe`) already exist in `src-tauri/data/tools.json` with `format` + `installKind` + `dest`. Any change to these entries must keep upstream parity: run `scripts/check-tools.sh` against `ALL_TOOLS` in `install.sh` and the converter set in `convert.sh`. Cite: `tools.json` `_note`, `docs/PLAN.md` L114–150.
- **[P0]** New `format` keys added for multi-file renderers (`aider-conventions`? currently `aider-conventions`/`windsurf-rules`/`kimi-agent`/`openclaw-workspace`/`vibe-toml`) must each map to a renderer in `render::render` dispatch; otherwise install stays gated (current behavior returns `unsupported`). Cite: `render/mod.rs::render` match, `unsupported()`.
- **[P1]** `tools.json` is upstream-owned (twin of `divisions.json`); app changes must not alter upstream-truth fields (`format`, `installKind`, `dest`, `scope`) without a corresponding upstream PR. Cite: `memory-bank/decisions.md` (twin contract).
- **[P1]** Dependency hygiene: `cargo.lock`/`package-lock.json` pinned; new deps require hash verification. Cite: `SECURITY.md`, `SEC-09`.
- **[P2]** Re-run the upstream parity test (`upstream_convert_sh_is_byte_identical_for_transform_tools`) extended to the new tools once renderers land. Cite: `render/mod.rs` tests.

## 5. Accessibility / WCAG for UI changes (tool enablement) — P1

- **[P1]** Enabling previously dimmed tools (Aider/Windsurf/Kimi/OpenClaw/Vibe) in the Tools panel must not regress keyboard navigation, focus order, or screen-reader labels; new "installable" state must be announced (aria-live or role=status). Cite: frontend tool list components (search `tools_list` consumers).
- **[P1]** Color-only state changes (recognized-only dimming → enabled) must have a non-color indicator (icon/text). Existing accent colors per tool must meet WCAG 2.1 AA contrast (4.5:1) on the app background. Cite: `tools.json::accent`.
- **[P2]** Error toasts for unsupported/failed installs must be readable and dismissible; no auto-dismiss under 5s for errors. Cite: `install/mod.rs` error returns.
- **[P2]** Multi-file dest reveal (`reveal_path`) already opens OS file manager; ensure it targets the first dest and is keyboard accessible. Cite: `install/mod.rs::reveal_path`.

## 6. Licensing of upstream converter output (MIT) — P1

- **[P1]** Renderers port `scripts/convert.sh` heredocs from the upstream `agency-agents` repo (MIT). The app must retain the upstream MIT license notice and attribution; rendered files carry no license header requirement, but the ported code must keep SPDX/credit comments where present. Cite: `render/mod.rs` module doc + `LICENSE`.
- **[P1]** No additional restrictions may be imposed on the converted output beyond upstream MIT; app's own code remains under its license. Verify `LICENSE` compatibility. Cite: repo `LICENSE`.
- **[P2]** If a renderer reuses a third-party template, add it to `THIRD_PARTY_NOTICES` / equivalent. Cite: (create if missing).

---

## Implementation gates (must pass before merge)

1. `render::render` + `render_with_hash` implemented for all five formats; `dests` already wired via `tools.json`. Add unit tests mirroring `antigravity_native_agents_shape_and_dests`.
2. `unsupported_tools_error` test updated: remove the five from the "no renderer" list; keep any still-unsupported (e.g. `hermes` plugin).
3. Path-traversal + slug allowlist tests added in `render` and `install`.
4. `cargo test` + `cargo clippy` clean; no new `Command`/shell in render module.
5. UI: tools panel shows enabled state; a11y smoke (keyboard + screen reader) recorded.
6. `scripts/check-tools.sh` (upstream) passes with no parity drift.
7. License notice verified in `LICENSE`/docs.

## Files to touch
- `src-tauri/src/render/mod.rs` (render dispatch + multi-file return shape)
- `src-tauri/src/install/mod.rs` (multi-file write/uninstall + roster accumulation)
- `src-tauri/data/tools.json` (already has entries; verify only)
- `src-tauri/src/util/fs.rs` (reuse atomic_write/read_capped)
- Frontend tool panel components (enablement + a11y)
- `docs/` + `LICENSE` (attribution)

**Owner**: engineering-backend-architect (render/install) + engineering-frontend-developer (UI) + support-legal-compliance-checker (this scan).
**Status**: Draft — pending human review before implementation (DENY-07).
