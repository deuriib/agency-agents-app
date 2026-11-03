# Agency Agents

> A native installer for AI agents.

[![License: MIT](https://img.shields.io/badge/License-MIT-yellow.svg)](./LICENSE)
[![PR Check](https://github.com/deuriib/agency-agents-app/actions/workflows/pr-check.yml/badge.svg)](https://github.com/deuriib/agency-agents-app/actions/workflows/pr-check.yml)
[![Latest release](https://img.shields.io/github/v/release/deuriib/agency-agents-app?display_name=tag&sort=semver)](https://github.com/deuriib/agency-agents-app/releases/latest)
[![Built with Tauri 2](https://img.shields.io/badge/Built%20with-Tauri%202-orange)](https://tauri.app)
[![macOS 13+](https://img.shields.io/badge/macOS-13%2B-lightgrey)](https://www.apple.com/macos)
[![Sponsor](https://img.shields.io/badge/♥-Sponsor-EC4899?logo=githubsponsors&logoColor=white)](https://github.com/sponsors/deuriib)

Agency Agents is a small, native app for browsing, installing, and tracking the agent personas from [`deuriib/agency-agents`](https://github.com/deuriib/agency-agents) across the AI coding tools you actually use.

It is full source, MIT-licensed, local-first, and does not run telemetry.

<picture>
  <source media="(prefers-color-scheme: dark)" srcset="landing/screenshots/dashboard-dark.png">
  <img alt="Agency Agents — Dashboard: install health, cross-tool coverage, and the catalog by division" src="landing/screenshots/dashboard-light.png">
</picture>

## Why This Exists

The `agency-agents` repo is a useful catalog of specialist AI agent personas, but every coding tool has its own agent format and install path. Every tool wants similar content in slightly different places — Claude Code, Codex, Cursor, Gemini CLI, opencode, Copilot, and seven more.

Agency Agents gives that catalog a native control surface:

- browse the agent catalog by division and role
- inspect the source persona before installing it
- install deterministic renders into supported tools
- track what the app wrote using a local ledger
- detect drift when a file was modified outside the app
- update, remove, or back up installs without guessing

The core idea is simple: AI tools do not share a package database, so the app keeps the local install database they are missing.

## Features

Agency Agents is organized around four pillars — **Agents** (who), **Tools** (how), **Teams** (which), and **Projects** (where):

- **Agents workspace** — searchable three-pane catalog, division and category filters, an install-state lens, a detail panel, and per-agent deployment controls.
- **Tools panel** — shows all recognized tools from the registry, detected installs, counts, versions where available, default targets, project installs, and bulk operations. Installable targets render in full; recognized-only targets appear dimmed.
- **Teams** — app-bundled preset teams plus your own saved teams; open a team for a detail panel with Deploy built in. (Teams replaces the earlier "Loadouts" concept; Agentfile export/import remains.)
- **Projects** — project-scoped installs with a dedicated panel and master/detail navigation, so a project gets exactly the agents and tools it needs.
- **Install tracking** — records every app-managed install with source hash, rendered hash, tool, destination, scope, and project path where relevant.
- **Reconciliation** — classifies installed files as current, outdated, modified, removed, or foreign by re-rendering canonical source and comparing bytes. The Dashboard surfaces what "needs attention," and the Agents pane filters to exactly those.
- **Auto-update** — checks a signed update manifest and installs new versions in place, verified against an embedded key, with one-click install + relaunch. Live for macOS (Apple Silicon + Intel) as of v0.2.0; opt-in and gated by Settings.
- **Tool registry** — tool knowledge lives in a single upstream-owned `tools.json` shared by the backend and frontend; adding a tool is editing one JSON entry, the entry shape is enforced by `src-tauri/data/_schema.json` (Draft 2020-12, IDE tooltips included), and installability is derived from whether the app ships a renderer for that tool's format. Rule harnesses follow the same declarative pattern in `src-tauri/data/rules-tools.json` (a verbatim copy of the upstream `agency-rules` `harnesses` object) plus `src-tauri/data/rule-formats.json` (the rule formats this app renders natively); a harness is installable iff its format appears there.
- **Dashboard** — install health, a Global-vs-Projects install sunburst, cross-tool coverage merged with the catalog-by-division view (linked hover), and deep links back into the workspace.
- **GitHub integration** — optional OAuth Device Flow for GitHub-backed app features. Tokens are stored in the platform keychain and are never returned to the frontend.
- **Offline-first catalog** — ships with a bundled corpus baseline and can use a local or managed clone of `agency-agents`.
- **Cross-platform shell** — Tauri 2 + Svelte 5 frontend with native macOS chrome and opaque native windows on Windows/Linux.

New to directing agents? See **[docs/USING-AGENTS.md](./docs/USING-AGENTS.md)** — the Playbook: how to get shipped, tested work out of the catalog (also in-app via the title-bar book icon).

### Rules

The **Rules** workspace (sidebar, `Cmd/Ctrl+7`) manages AI rules from the [`deuriib/agency-rules`](https://github.com/deuriib/agency-rules) catalog: each rule is authored once in neutral Markdown and compiled natively for the harness you install it into.

- **Catalog** — every rule in the managed `agency-rules` corpus, searchable and filterable by harness and category. A rule that fails frontmatter validation is still listed, badged, and explains why it cannot be installed instead of silently disappearing.
- **Compiled preview** — the authored body plus the exact bytes each harness would receive, with the resolved destination path. Nothing is written while previewing.
- **Native compilation** — the four rule renderers are Rust, not a bundled Node or CLI dependency, and are byte-for-byte identical to the upstream `agency-rules` compiler (golden fixtures), so what the preview shows is what lands on disk.
- **Install** — per rule, per harness, with user or project scope. Project scope requires a folder you pick; the app never writes to an arbitrary directory and does not create a Projects entry.
- **Ledger tracking** — rule installs are recorded in the same local ledger as agents, with a domain discriminator. Installed state survives a restart, and agent reconciliation never counts a rule as an agent. A ledger that cannot be read is reported as an error rather than treated as empty, so a permission problem cannot make the app forget what is installed, and an install preflights the ledger so a failure never leaves an untracked file behind.
- **Uninstall** — removes the file(s) the app wrote and the ledger row. It never deletes a directory (pi-agent keeps its now-empty per-rule folder) and never removes the OpenCode plugin entry. If the project a rule was installed into has since been deleted, or the harness is no longer in the contract, uninstall still clears the ledger row instead of failing forever.
- **OpenCode plugin registration** — installing an OpenCode rule merges the `opencode-rules` plugin into `opencode.json` (project root or `~/.config/opencode`), preserving unrelated keys. Uninstall deliberately leaves the plugin entry in place.

#### Rule harnesses

Rule harnesses are a separate contract from the agent install targets above — `src-tauri/data/rules-tools.json` is the upstream `agency-rules` `harnesses` object, and `src-tauri/data/rule-formats.json` lists the rule formats this app renders natively. The corpus itself is a second managed catalog: a bundled baseline ships in the app for a first launch offline, and **Sync** / **Refresh** downloads the `agency-rules` tarball, swaps the corpus atomically, and rebuilds the index (Offline Mode gates it; there is no runtime `git` dependency).

| Harness | Output format | User destination | Project destination |
| --- | --- | --- | --- |
| Google Antigravity | `md-yaml-trigger` | `~/.gemini/config/rules/*.md` | `.agents/rules/*.md` |
| Pi Coding Agent | `md-rules-dir` | `~/.pi/agent/rules/*/RULES.md` | `.pi/rules/*/RULES.md` |
| oh-my-pi (omp) | `md-or-mdc` | `~/.omp/agent/rules/*.md` | `.omp/rules/*.md` |
| OpenCode (via opencode-rules) | `md-plugin` | `~/.config/opencode/rules/*.md` | `.opencode/rules/*.md` |

**Not part of this milestone**: rule drift detection and the five-state reconciliation, rule auto-update, assigning rules to Projects, rule metrics on the Dashboard, and a user-local custom-rules directory. Rule installs are catalogued, previewed, tracked, and removable — they are not yet reconciled.

## Supported Install Targets

The app currently installs to thirteen renderer-backed targets that have deterministic byte parity with the upstream `agency-agents` converter. Both columns are derived from `src-tauri/data/tools.json` — the registry reads the same file — and `npm run test:scripts` fails when this table drifts from it:

| Tool | Scope Today | Output |
| ------ | ------------- | -------- |
| Claude Code | user + project | `~/.claude/agents/*.md`, `.claude/agents/*.md` |
| Codex | user + project | `~/.codex/agents/*.toml`, `.codex/agents/*.toml` |
| Gemini CLI | user + project | `~/.gemini/agents/*.md`, `.gemini/agents/*.md` |
| GitHub Copilot | user + project | `~/.copilot/agents/*.md`, `~/.github/agents/*.md`, `.github/agents/*.md` |
| Qwen Code | user + project | `~/.qwen/agents/*.md`, `.qwen/agents/*.md` |
| Cursor | project | `.cursor/rules/*.mdc` |
| opencode | user + project | `~/.config/opencode/agents/*.md`, `.opencode/agents/*.md` |
| Osaurus | user | `~/.osaurus/skills/agency-*/SKILL.md` |
| Antigravity | user + project | `~/.gemini/config/agents/*.md`, `.agents/agents/*.md` |
| ZCode | user + project | `~/.zcode/agents/*.md`, `.zcode/agents/*.md` |
| DeepSeek Harness | user + project | `~/.dsh/skills/agency-*/SKILL.md`, `.dsh/skills/agency-*/SKILL.md` |
| Pi Agent | user + project | `~/.pi/agent/agents/*.md`, `.pi/agents/*.md` |
| Oh My Pi | user + project | `~/.omp/agent/agents/*.md`, `.omp/agents/*.md` |

Aider, Windsurf, Kimi, OpenClaw and Mistral Vibe ship install contracts in the catalog but are recognized-only: no `convert.sh` port ships here yet, and the install path refuses them rather than writing a file no CLI can read. Hermes is a single router plugin installed by its own CLI, so it is never a per-agent target.

## What This Isn't

- Not an agent runtime. The app installs personas into other tools; it does not execute them.
- Not a replacement for the `agency-agents` repo. The repo remains the source catalog.
- Not a telemetry product. There are no analytics SDKs, user tracking, or accounts required for core use.
- Not a shell command bridge. The frontend cannot construct arbitrary shell commands.

## Install

Grab the build for your platform from the [latest release](https://github.com/deuriib/agency-agents-app/releases/latest):

- **macOS** (Apple Silicon & Intel) — signed + notarized `.dmg`, macOS 13+.
- **Linux** (x86_64) — `.deb`, `.rpm`, or the portable `.AppImage`.
- **Windows** (x64 & ARM64) — `.exe` installer (not code-signed yet; SmartScreen → *More info → Run anyway*).

Or on macOS via Homebrew:

```sh
brew tap msitarzewski/agency-agents
brew trust --cask msitarzewski/agency-agents/agency-agents
brew install --cask agency-agents
```

For local review, use the development app:

```sh
npm install
npm run tauri dev
```

For a signed release build on macOS, see [docs/BUILD.md](./docs/BUILD.md).

## Build From Source

Prerequisites:

- [Rust](https://rustup.rs/) stable
- [Node.js 22+](https://nodejs.org/) and npm
- Xcode Command Line Tools on macOS: `xcode-select --install`
- Full Xcode only when regenerating the macOS Liquid Glass icon assets

Then:

```sh
git clone https://github.com/deuriib/agency-agents-app
cd agency-agents-app
npm install
npm run tauri dev
npm run check
cargo test --manifest-path src-tauri/Cargo.toml --lib
npm run build
```

Or, with [mise](https://mise.jdx.dev/) — the pinned toolchain and the CI
checks in one place:

```sh
git clone https://github.com/deuriib/agency-agents-app
cd agency-agents-app
mise install       # node + rust from mise.lock
mise run setup     # npm ci
mise run dev
mise run ci        # fmt, clippy, rust tests, svelte-check, build
```

The Phase C local QA batch is:

```sh
npm run build:phase-c
```

Use the full VM-assisted batch when the configured Ubuntu/Windows test environments are available:

```sh
npm run build:phase-c:full
```

## Architecture

A Tauri 2 shell hosts a SvelteKit + Svelte 5 frontend in the system WebView. The Rust backend owns the catalog, renderer, install ledger, reconciliation, GitHub integration, settings, and updater boundary.

The catalog comes from `agency-agents`, either as:

- a bundled baseline inside the app
- a managed local clone at `~/.agency-agents`
- a user-selected clone, such as `/Users/michael/Software/AgentLand/agency-agents`

A catalog carries two kinds of content: the agent personas the app indexes (`<division>/<slug>.md`) and the repo-level content it reads directly — `divisions.json` (division metadata) and `strategy/runbooks.json` (the NEXUS scenario runbooks that power the Runbooks pillar). Snapshot provisioning extracts both; a clone or snapshot that predates `strategy/` shows Runbooks empty until the catalog is refreshed.

The Rules workspace reads a second managed corpus, `deuriib/agency-rules`, extracted to its own `<app-data>/rules-corpus` with its own bundled baseline, tarball refresh, and `state/rules-index.json`. It deliberately shares neither the agent corpus directory nor its index: the two repositories refresh on independent schedules.

Rendering is native Rust, deterministic, and tested against the upstream `scripts/convert.sh` outputs for the supported transform tools. The app does not shell out to converter scripts at runtime.

Important implementation areas:

- [src-tauri/src/corpus/mod.rs](./src-tauri/src/corpus/mod.rs) — catalog source, indexing, refresh, category discovery
- [src-tauri/src/render/mod.rs](./src-tauri/src/render/mod.rs) — per-tool deterministic rendering and destination paths
- [src-tauri/src/install/mod.rs](./src-tauri/src/install/mod.rs) — install, uninstall, ledger, detection, reconciliation
- [src-tauri/src/rules/](./src-tauri/src/rules/) — rule-harness contract, parsing and validation, native rule renderers, rules corpus, commands
- [src/lib/components/AgentsWorkspace.svelte](./src/lib/components/AgentsWorkspace.svelte) — main browse/install surface
- [src/lib/components/Rules.svelte](./src/lib/components/Rules.svelte) — Rules workspace: catalog, detail, compiled preview, install controls
- [src/lib/components/ToolsView.svelte](./src/lib/components/ToolsView.svelte) — tool status and bulk operations

Memory Bank design context lives under [memory-bank/](./memory-bank/). Start with [memory-bank/projectbrief.md](./memory-bank/projectbrief.md), [memory-bank/systemPatterns.md](./memory-bank/systemPatterns.md), and [memory-bank/NEXT-SESSION.md](./memory-bank/NEXT-SESSION.md).

## Network Posture

Core browsing and install tracking are local. Network access is explicit and gated by Settings.

Known outbound paths:

- GitHub/codeload/raw GitHub endpoints for refreshing the `agency-agents` catalog when the user requests or enables it.
- GitHub OAuth Device Flow when the user chooses to sign in.
- GitHub API calls for optional GitHub-backed app features.
- The app updater manifest and release artifacts when update checks are enabled.

No telemetry, crash reporting, advertising pixels, or product analytics are included.

## Security

Agency Agents uses typed Tauri IPC commands and avoids `tauri-plugin-shell`. File writes are restricted to known install destinations, app state, backups, and user-selected paths. Modified installed files are backed up before destructive operations.

Report vulnerabilities using [SECURITY.md](./SECURITY.md).

## Contributing

Contributions are welcome. See [CONTRIBUTING.md](./CONTRIBUTING.md).

The highest-value areas before 1.0 are:

- verified tool-target manifest shared with the AA repo
- additional project-scope install targets
- multi-file renderer support for Aider, Windsurf, OpenClaw, Antigravity, and Kimi once their target formats are verified
- Windows/Linux packaging validation
- GitHub issue/discussion integrations

## License

[MIT](./LICENSE). Do whatever you want with this.

## Acknowledgments

- [Agency Agents](https://github.com/deuriib/agency-agents) — the source catalog and upstream converter/install scripts. The app contributes its transforms back upstream: v0.2.0's Osaurus integration and the shared `tools.json` tool manifest (the twin of `divisions.json`) landed there first.
- [Tauri](https://tauri.app) — native app shell without the Electron footprint.
- [Svelte](https://svelte.dev) — the frontend runtime.

## Support The Project

If Agency Agents saves you time, consider [sponsoring on GitHub](https://github.com/sponsors/deuriib). Sponsorship is optional and does not unlock a paid tier.
