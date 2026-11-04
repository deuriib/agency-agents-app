# Decisions (ADRs) — Agency Agents

## 2026-11-04 — Gate `main` first, then stop re-running the PR graph on every push

**Context.** Issue #90. `pr-check.yml` ran all eight jobs on `pull_request` and again
on `push: main`, so every merge repeated the graph on a tree that had just been
tested: 243s of job time and 9 billed minutes per merge (run `41754881874`), plus a
duplicate 9s gitleaks scan in `41754881875`.

The obvious fix — delete `push: [main]` — was blocked by an honest counter-argument.
The merge commit is a new SHA, and nothing forced a PR to be rebased before merging,
so a PR merged while behind `main` produced a tree that was never tested. `main` was
not hardened at all: `gh api .../rulesets` returned `[]` and `branches/main/protection`
404'd, which made the push-to-main run the only post-merge signal. That signal had
already earned its keep — the `cargo-lock` job failed on `main` twice
(`41744617675`, `41736166805`) after releases merged with a stale lock.

**Decision.**

1. **Gate first, remove second.** Ruleset `main: PR gate` (`26500514`) targets
   `~DEFAULT_BRANCH` with `required_status_checks` (the eight `pr-check.yml` jobs plus
   `gitleaks`), `strict_required_status_checks_policy`, `non_fast_forward` and
   `deletion`. It went live before the workflow change, so the PR that removed the
   duplicate was validated by the rule it depends on.
2. **Pin every required context to `integration_id: 15368`** (GitHub Actions). A check
   *name* is not a trust boundary: without the pin, a third-party App could publish a
   green check run called `Rust tests` over a red PR.
3. **`allow_update_branch` → `true`.** It was `false`, which would have turned
   "require branches up to date" into a local rebase plus force-push on every stale PR.
   Enabled, the requirement is a button. A gate that is annoying is a gate that gets
   switched off within a month.
4. **Three jobs become `pull_request`-only**: `rust-lint`, `rust-test`, `frontend` —
   the ones that compile or run `npm ci`.
5. **Four jobs keep running on `push: main`** as a canary: `rust-fmt`, `cargo-lock`,
   `scripts`, `vendor` — manifest and pinned-artifact reads, 39s total.
6. **`gitleaks` becomes the ninth required check** and loses `push: [main]`; it keeps
   `pull_request` and the weekly `schedule`. Without the required check, dropping the
   push trigger would leave a secret unseen until Monday's cron — the cleanup, not the
   gate.
7. **`release-please.yml` untouched.** `push: main` is its trigger, not a duplicate.
   `pages.yml` and `sync-labels.yml` are path-filtered deploys.

**Alternatives rejected.**

- **Deleting `push: [main]` first, gating later.** Would delete the only post-merge
  check while nothing enforced green PRs.
- **Classic branch protection.** A ruleset is declarative, exportable and the direction
  GitHub is moving; `branches/main/protection` is the legacy surface.
- **A merge queue.** Correct for a busy repo, but it needs `merge_group` plumbing in
  every workflow, and `strict_required_status_checks_policy` already guarantees an
  up-to-date head on a squash-only repository.
- **A bypass for the admin role.** Would restore exactly the hole the issue describes:
  a path that lands a commit on `main` nobody validated. The escape hatch is editing the
  ruleset, which is deliberate and visible.
- **The canary in its own workflow file.** Honours AC2's literal wording, but the file
  would duplicate the `cargo-lock` and `vendor` steps and drift silently from
  `pr-check.yml`. Three `if:` guards degrade safely instead: a job added without one just
  runs on both events, which is the old behaviour.

**Consequences.** Per merge, 252s of job time becomes 39s (−85%) and 10 billed minutes
become 4. `main` can no longer be updated without a green PR — including by a direct
push, which returns `GH013: Required status check ... is expected` (verified against a
sacrificial ruleset on a throwaway ref, not assumed), and therefore a correction to a red
`main` must also be a PR. The gate is **not** unskippable: it declares no
`bypass_actors`, but a repository admin can still edit or delete the ruleset or use the
API administrator override. `CONTRIBUTING.md#main-is-gated` says so plainly, because a
hardening claim the first curious maintainer can disprove costs the rest of the document
its credibility. Runner minutes on this public fork were never billed, so the win is wall
clock, concurrency slots and history noise — and `Release Please` still costs 760s of job
time per release merge, which is where any real runner-time work would go.

**References**: `.github/workflows/pr-check.yml`, `.github/workflows/gitleaks.yml`,
`CONTRIBUTING.md`, PR #99 (`d186253`), probe PR #100, runs `41754881874` / `41759214920`,
`tasks/2026-11/261104_drop-push-main-graph.md`.

## 2026-11-03 — The catalog and rule contracts are pinned submodules, generated into the tree

**Context.** Issue #72. The app consumed `agency-agents` and `agency-rules` by
hand-vendored copy: a 4.6 MB `resources/corpus-baseline/` with no record of the commit
it came from, a 52 KB `resources/rules-baseline/`, a `data/tools.json` copy that had
drifted from upstream in five `detect.dirs`, and a `data/agency-categories.json` that
described itself as a mirror of `divisions.json` while using a different top-level key.
Nothing tested the relationship, so a re-vendor would silently revert deliberate edits,
and a deleted upstream agent kept shipping. Runtime refresh fetched `refs/heads/main`,
unpinned, and the parity checks needed a sibling checkout via environment variables.

**Decision.**

1. **Git submodules pinned to release tags**, under `vendor/` so the app's own `src/`,
   `docs/` and `scripts/` keep their meaning: `vendor/agency-agents` (v1.6.3) and
   `vendor/agency-rules` (v0.9.0). The submodule pointer is the catalog version.
2. **One generator, one verifier.** `scripts/sync-vendor.mjs` derives every embedded
   artifact from the checkouts; `--check` (CI job `vendor`, `mise run check-vendor`)
   fails when any generated file is stale, missing or extra. All four workflows that
   build or test the app check out submodules.
3. **A closed overlay, not a silent divergence.** `src-tauri/data/tools-overlay.json`
   is the only sanctioned difference from upstream: one named field per tool with a
   `_why`, rejected when it stops differing (dead override) or names an unknown field.
   Detection roots are the genuine case — upstream lists agent *destination* dirs,
   the installer must recognise *installed surfaces*.
4. **No mirror files.** `data/agency-categories.json` is gone; the Rust floor now
   embeds `vendor/agency-agents/divisions.json` directly, so there is no second schema
   and no `_note` overstating a relationship the code did not have.
5. **Baselines are generated copies, not read-through.** Tauri bundles a resource tree,
   so the corpus stays a physical baseline — but one produced by the generator and
   verified by CI, which removes the drift class without making the crate depend on a
   build-time file walk.
6. **Fail fast without the submodules.** `src-tauri/build.rs` stops before
   `tauri_build` and prints the `git submodule update --init --recursive` line.

**Alternatives rejected**:
- *Lockfile + verified tarball*: reproducible and archive-friendly, but keeps three
  trees separate and adds a sync step — the opposite of working on everything in one
  checkout.
- *`git subtree`*: no clone friction, but 4.6 MB recommitted and review noise on every
  catalog change.
- *Runtime-only* (drop the baseline): breaks offline first launch.
- *Read `tools.json` through a runtime merge in Rust and TypeScript*: two
  implementations of the same overlay, and the Node guards would need a third. One
  generated file keeps every consumer unchanged.
- *Push the `detect.dirs` correction upstream instead of an overlay*: right long term,
  but it couples this PR to an `agency-agents` release; the overlay keeps the
  divergence explicit and testable today.

**Consequences**: contributors must initialise submodules (mitigated by the preflight
message and CONTRIBUTING), GitHub source archives cannot be built (stated in the
README), and a catalog fix reaches users only when the app re-releases or the user
refreshes. Pinning surfaced a real upstream defect: `finance-profit-first-strategist.md`
shipped frontmatter that is invalid YAML. It was fixed upstream
(`deuriib/agency-agents#47`, released as v1.6.3) and the pin now indexes all 298 files.
The pinned tag also predates the
catalog's move from `scripts/convert.sh` to `python -m agency_agents` +
`convert-outputs.sha256`, so the bash parity test now reports "skipping"; a
manifest-based oracle replaces it in #76.

**References**: `scripts/sync-vendor.mjs`, `src-tauri/build.rs`,
`src-tauri/data/tools-overlay.json`, `.gitmodules`, `.github/workflows/pr-check.yml`,
`CONTRIBUTING.md`, `docs/BUILD.md`, `tasks/2026-11/261103_pinned-catalog-submodules.md`.

## 2026-11-03 — Rules compile natively and share the agent ledger

**Context.** Issue #29 folds `deuriib/agency-rules` into the app: write a rule once,
compile it to four harness formats (Antigravity, pi-agent, oh-my-pi, OpenCode), then
track and remove it. The upstream project ships a Node CLI whose renderers are the
reference implementation. The app had three plausible shapes: invoke that CLI, design
a plugin system, or port the renderers.

Two constraints decided it. First, `systemPatterns.md` §3 already fixes the contract
for agents: conversion is done natively in Rust, and the reference converter is a
*spec*, never a runtime dependency — a Tauri app that bundles Node to convert Markdown
is a much larger dependency surface than four small renderers. Second, the app's core
differentiator is being the database: `rendered_hash` is what makes reconciliation
possible, so the output bytes have to be stable and defensible.

**Decision.**

1. **Native Rust port** of the four renderers, with a purpose-built emitter that
   reproduces `yaml@2`'s scalar decisions. Byte-parity with the reference compiler is
   the contract, proven by golden fixtures generated from a fresh upstream run plus an
   adversarial matrix.
2. **A second managed corpus** for `agency-rules` (`rules-corpus/`,
   `state/rules-index.json`), mirroring the agent corpus model: bundled baseline,
   tarball refresh, atomic swap, independent schedule.
3. **One ledger, a domain discriminator.** Rule rows carry `domain: "rule"` and are
   filtered out of every agent-facing surface, in both directions, rather than living
   in a second file that would drift from the first.
4. **Fail closed on shapes the emitter cannot reproduce.** A schema-valid rule can
   carry a multi-line value that upstream folds; writing bytes that differ from
   upstream would make `rendered_hash` lie, so those rules are refused with a visible
   reason instead of installed.

**Why not the alternatives.**
- *Invoke the `agency-rules` CLI* — rejected: bundles a Node runtime into a Tauri app,
  contradicts §3, and makes the build depend on a second toolchain.
- *Plugin system with `agency-rules` as the first plugin* — rejected: no concrete pain
  point justifies the abstraction (ENG-13), and there is exactly one plugin.
- *A second ledger file for rules* — rejected: two sources of truth for "what is
  installed", and every consumer would have to remember to read both.
- *One id scheme for both domains, now* — deferred (#68): the rule contract mirrors
  upstream verbatim, so unifying `{{slug}}`/`{slug}` and `pi-agent`/`piAgent` requires
  a coordinated upstream change plus a ledger alias migration, not a rename.

**Consequence.** Rules and agents now share one corpus model, one ledger and one
render-time contract. The cost is that the renderer is a compatibility surface: a
reference-compiler change needs a corresponding fixture refresh, and the parity run is
load-bearing (`docs/BUILD.md`). Fixtures must be generated from a fresh compiler run,
never copied from the upstream `integrations/` tree, which is gitignored generated
output and was already stale when this landed.

**References.** PR #61 (`1ab9453`), `docs/rules-integration-spec.md`,
`docs/rules-integration-tasks.md` (rulings A1–A14, C1–C11).

## 2026-11-02 — Snapshot provisioning mirrors the catalog tree, not an allow-list of divisions

**Context.** The Runbooks pillar stayed empty (#58) because the app's snapshot
provisioning wrote only division directories. Discovery had been keyed on
`scripts/convert.sh`, which the catalog removed on 2026-10-26; the extractor then
fell back to the bundled division list and dropped everything else the tarball
carried — including `strategy/runbooks.json` and `divisions.json`. The result
passed `looks_like_catalog`, so nothing ever repaired it.

**Decision.** Two rules.
1. **Discovery reads the catalog's own manifest** (`divisions.json`), never a
   script's side effects; the old `convert.sh` parse stays only as a legacy
   fallback.
2. **Extraction preserves repo-level content verbatim** — `divisions.json`,
   `strategy/**`, `scripts/**`, plus division personas with their nested paths —
   instead of an allow-list of directories the app indexes.

`provision_managed` also treats "divisions present, `strategy/` absent, no
`.git`" as a stale snapshot and re-extracts, so a bad root is repaired rather
than trusted forever.

**Why not the alternatives.**
- *Keep the allow-list and add `strategy/`* — rejected: the next repo-level file
  the app reads hits the same silent drop. The failure mode must not be "a
  directory nobody remembered to allow".
- *Vendor the runbook manifest into the bundled baseline* — rejected here:
  content refresh, not a bug fix. `seed_from_baseline` now carries `strategy/`
  whenever the baseline has it, so the next re-vendor needs no code change.
- *Treat a missing manifest as "not synced" forever* — rejected: that is exactly
  the misleading CTA that hid the defect. Absent is empty; unreadable is an error.

**Consequence.** The on-disk catalog now matches the repo it claims to mirror,
and "empty Runbooks" means only one thing: this catalog ships no `strategy/`.

## 2026-09-12 — Strip AppImage bundle paths from every child process, rather than patching git

**Context.** The v0.3.0 AppImage could not clone the catalog on any host that was not the build host. linuxdeploy's
`AppRun.wrapped` exports `LD_LIBRARY_PATH` with the bundle's directories first and the caller's original appended,
and saves no copy of the original anywhere. Children inherit it, so a host binary loads bundle libraries it was not
built against. Confirmed by unsquashing the shipped AppImage and reading `AppRun`, `apprun-hooks/…-gtk.sh` and the
strings in `AppRun.wrapped`.

**Decision.** One helper, `util::proc`, applied at **every** spawn site rather than a targeted fix in `run_git`.
It removes only path entries under `$APPDIR`, leaving the host's own search path intact, and is a no-op when
`$APPDIR` is unset.

**Why not the alternatives.**
- *Fix `run_git` only* — rejected: `probe_version` and `reveal_path` spawn host binaries through the same
  environment. Tool detection on Linux was plausibly degraded by this too. A bug report names a symptom; grep for
  the siblings (`grep -rn "Command::new"`).
- *Restore the original `LD_LIBRARY_PATH`* — impossible: AppRun keeps no copy. Removal is the only correct option.
- *Clear the whole environment* — rejected: too blunt, would break legitimate inherited config.
- *Strip `XDG_DATA_DIRS`/`GTK_*` wholesale* — rejected: AppRun *prepends* to those, so the host halves are real and
  must survive. We filter per entry.

**Consequence.** Bundle-vs-host library conflicts are closed off for all current and future child processes. The
rule generalizes: **anything an AppImage spawns must have the bundle stripped from its environment first.**


### 2026-06-05: Fork brew-browser structurally
**Status**: Approved. **Context**: brew-browser is a proven, signed, shipping Tauri 2 +
Svelte 5 native macOS app that is "a thin respectful frontend over a CLI." Agency Agents
is the same pattern over the agency-agents repo. **Decision**: rsync the scaffold, keep
the entire shell/UI/build/updater infra, replace only the brew *domain*. **Consequences**:
~90% of non-domain code reused; fastest path to a real signed app.

### 2026-06-05: Plan B — native Rust install engine (not shell-out)
**Status**: Approved. **Context**: brew-browser shells out to `brew` because brew does
heavy lifting (downloads, deps, compile). Our "install" is just transform-frontmatter +
copy-file. **Alternatives**: (A) shell out to repo's `install.sh`/`convert.sh` — adds
runtime bash/python dep, non-deterministic output. **Decision**: reimplement conversion +
install natively in Rust; `convert.sh` is the reference spec. **Consequences**: self-
contained, cross-platform, and — critically — **deterministic output we can hash**, which
is the prerequisite for state tracking. Load-bearing.

### 2026-06-05: Corpus-copy model (own the repo locally)
**Status**: Approved. **Context**: the catalog (agency-agents) is small (3.4MB), git-
versioned, and changes constantly. **Decision**: maintain our own working copy in app
support, seeded from a bundled baseline, refreshed from the GitHub **tarball** (no runtime
git). Derive `corpus-index.json` (hash index) from it. **Consequences**: one decision
unifies catalog + updates + provenance + trending (git history). Commit SHA = version.

### 2026-06-05: State tracking — ledger reconciled against disk (we ARE the database)
**Status**: Approved. **Context**: AI tools have no install registry; `install.sh` copies
and forgets. **Decision**: maintain a ledger (`installs.json`) and reconcile it against
disk + corpus-index into 5 states (Current/Outdated/Modified/Removed/Foreign). Two hashes:
`source_hash` (version identity) + `rendered_hash` (local-edit detection). **Consequences**:
this is the app's core differentiator — cross-tool agent state nobody else has.

### 2026-06-05: Provenance by hash-match only — never mutate agent files
**Status**: Approved. **Alternatives**: stamp `x-agency-source` into frontmatter (rejected:
mutates content, breaks TOML/.mdc/rules formats). **Decision**: identify "ours" by slug +
re-render hash-match against corpus-index; offer an explicit **Adopt** for recognized
Foreign files. **Consequences**: zero content mutation; respects every tool's format.

### 2026-06-05: Both scopes (user-global AND project-scoped), fully tracked
**Status**: Approved. **Decision**: user-global tools use fixed `~/…` dests; project-scoped
tools install into any dir and are tracked per `project_path` via a Projects registry.

### 2026-06-05: vulns→Quality, services→Tools, Snapshots→Loadouts
**Status**: Approved. **Decision**: repurpose brew-browser's opt-in vuln scanner as an
opt-in lint+originality scanner (agency-agents ships `lint-agents.sh` +
`check-agent-originality.sh`); `brew services` view becomes per-tool deployment management;
Brewfile snapshots become "Agentfile" loadouts.

### 2026-06-05: Agent catalog = 210 personas / 16 categories (not 251)
**Status**: Approved. **Context**: the agency-agents repo has ~251 `.md` total, but many are
docs. The corpus parser's real-baseline test revealed only files with `name:` frontmatter are
agents. **Decision**: the catalog is the **210** agent personas across **16** categories. The
repo's `strategy/` (NEXUS playbooks/runbooks) and `examples/` (multi-agent workflow walkthroughs)
are documentation, excluded from `CATEGORY_DIRS`. **Consequences**: honest headline count (210);
nested agents (game-development/unity, strategy subdirs) are flattened to their top category during
seeding so none are undercounted. **Future**: the NEXUS playbooks + workflow examples are good
content — candidate for a separate "Playbooks/Workflows" section later, not the agent catalog.

### 2026-06-05: Restore brew's real bundled data until the brew domain retires
**Status**: Approved. **Context**: Phase 0 swapped brew's bundled catalog for empty placeholders to
compile; that broke 14 brew-domain tests. **Decision**: restore brew's real `data/` files so the
not-yet-replaced brew domain stays green (always-green principle); delete them when brew
catalog/enrichment/categories modules are removed. Corpus uses its own `agency-categories.json`.

### 2026-06-14: Renderer parity is a tested contract, not an assumption
**Status**: Approved. **Context**: the `current`/Diff/Update state model assumes Rust `render/` output
is byte-identical to the upstream `scripts/convert.sh` for transform tools; a single newline drift would
make every CLI-installed Cursor/Codex/Gemini/opencode/qwen agent falsely read `foreign`/`modified`.
**Decision**: encode the converter's exact shell semantics in `render/mod.rs` (`source_field` =
`lib.sh#get_field` literal-field extraction with quotes preserved, `source_body` = awk +
command-substitution newline handling, `slugify`, `output_slug` filename rules) and enforce parity with
an `--ignored` test that shells out to the REAL converter and diffs every transform tool byte-for-byte.
**Consequences**: parity is now proven (232 agents × 5 tools = 1160/1160 identical) and regressions are
caught; the test must be re-run after any converter or catalog change (`npm run build:phase-c`).

### 2026-06-14: Uninstall is recoverable (backup-first), byte-identical needs no backup
**Status**: Approved. **Context**: quick ✕ / bulk Delete deleted files with no backup, unlike
Update/Restore. **Decision**: `remove_agent_files` runs a backup-first pass — modified/divergent files
back up to `backups/` BEFORE any deletion; byte-identical/canonical files need no backup (re-installable);
if a backup fails, the delete ABORTS and the original is preserved (a preservation failure can never
strand a half-removed agent). **Alternatives**: keep deletion final (rejected — data loss for divergent
agents). **Consequences**: the ✕ is now reversible for the cases that matter, with full test coverage.

### 2026-06-14: First release = v0.1.0, manual DMG, auto-update deferred
**Status**: Approved (plan only — NOT cutting yet). **Context**: all three manifests already read
`0.1.0`; signing + notarization are proven; the updater pubkey is real but the endpoint
(`agency-agents-app.zerologic.com/updater.json`) is not provisioned. **Decision**: ship v0.1.0 as a
signed + notarized `.dmg` for manual download, built with `SKIP_UPDATER=1`; defer auto-update to a later
release once the endpoint serves a manifest. **Out of scope** (documented as known limitations):
auto-update, multi-file renderers, Windows/Linux runtime verification, local-runtime target.
**Runbook**: `docs/BUILD.md#Release Checklist`. **Consequences**: fastest path to a real first release;
0.1.0 users update manually, but the shipped pubkey lets a later 0.1.x flip auto-update on. **Note**:
we are NOT cutting now — knocking out final pre-release issues first.

### 2026-06-15: Repurpose the inherited "Activity" surface into a usage journal
**Status**: Approved. **Context**: AA inherited brew-browser's "Activity" view (Sidebar ⌘4 + components +
223-line store) for streaming long-running `brew` jobs — but AA installs are instant native file writes, no
backend emits `AppStreamEvent`, so the section was fully built yet permanently EMPTY. **Decision**: keep the
surface but repurpose it as a frontend **journal** of discrete agent actions (install/uninstall/update/track/
bulk + default-target switch), logged from `install.svelte.ts`, persisted in localStorage, clearable. Delete
the dead streaming machinery (`AppStreamEvent`, `ActivityJob`, `ActivityDrawer`, the dead error codes).
**Alternatives**: remove Activity entirely (rejected — AA has plausible future long-running ops: catalog
clone/pull, updater download, bulk reconcile, which could stream into it later); wire real streaming now
(rejected — bigger scope). **Consequences**: turns dead weight into a useful history; localStorage is fine
(it's a UX journal, not a system of record — the ledger remains the source of truth). Future option: a
backend `activity.json` if it must survive webview-data clears.

### 2026-06-15: `.cargo/config.toml` to pass the tauri feature-gate on bare cargo
**Status**: Approved. **Context**: the cross-platform config split (PR #1) keeps `macOSPrivateApi` only in
`tauri.macos.conf.json`, merged by the Tauri CLI — but bare `cargo test`/`build`/CI read only base
`tauri.conf.json`, so `tauri-build` rejects the `macos-private-api` Cargo feature (fails on fresh checkout;
hidden locally by a warm build-script cache). **Decision**: a repo-root `.cargo/config.toml` sets
`TAURI_CONFIG='{"app":{"macOSPrivateApi":false}}'` for bare cargo invocations. **Alternatives**: put
`macOSPrivateApi` back in base config (rejected — adding it did NOT satisfy the gate empirically; the gate is
bypassed only when `TAURI_CONFIG` is set, and only with the `false` value). **Consequences**: bare cargo is
green from cold; the Tauri CLI sets its own process-env `TAURI_CONFIG` (precedence over `[env]`), so real
`tauri dev`/`build` use the merged config (`macOSPrivateApi: true`) — verified `tauri dev` launches clean.

### 2026-06-22: Updater host = `agencyagents.app`; dedicated agency signing key (resolves the OPEN host ADR)
**Status**: Approved (v0.2.0). **Context**: the prior OPEN entry assumed the endpoint would be
`agency-agents-app.zerologic.com` and that the embedded pubkey was a one-off real key. Tracing the live
build host settled both. **Decision (host — the important detail)**: the updater endpoint is
**`https://agencyagents.app/updater.json`**, NOT the `…zerologic.com` host the UI/docs/source comments
had drifted to. It is already in `tauri.conf.json` `endpoints` + the CSP `connect-src`, and Caddy on
`umacbookpro` serves `agencyagents.app` from `~/Sites/agency-agents/` (a sibling vhost to the live
`brew-browser.zerologic.com/updater.json`, so the file_server pattern is proven). Publishing is an
`rsync` of `dist/updater.json` to `umacbookpro:Sites/agency-agents/updater.json`. **Decision (key)**:
the embedded pubkey was byte-identical to brew-browser's shared key (id `7335DD0F`); swapped to a
**dedicated agency minisign key** (id `ABF5AFD8`) generated on the build machine — clean signing
isolation, done now while NO updater client is live (endpoint never served a manifest under
`SKIP_UPDATER`), so there is zero continuity cost. Private key + `signing.env` live at
`~/.config/agency-agents-app/` (chmod 600, outside the repo); Apple notarization uses the Developer ID
identity in the login keychain. The Rust `UPDATER_PUBKEY` const (`lib.rs`) is kept in sync with
`tauri.conf.json` for documentation only — verification is driven entirely by the conf value the
`tauri-plugin-updater` reads at startup. **Consequences**: v0.1.0 users (old embedded pubkey, and no
manifest ever existed) upgrade to v0.2.0 manually; auto-update is real from v0.2.0 forward. **Remaining
before live**: build without `SKIP_UPDATER`, run `tools/release/publish-manifest.sh`, host the signed
`.app.tar.gz` + manifest. Until then the updater ships present-but-disabled.

### 2026-06-16: Defer Windows code signing for v0.1.0
**Status**: Approved. **Context**: v0.1.0 is a Mac-first manual release (signed/notarized DMG); Windows is
build-validated by the Phase C matrix but not a committed shipping artifact. An unsigned Windows `.exe`
trips Defender SmartScreen ("Windows protected your PC" / unknown publisher). Removing that warning needs
both a trusted-CA signature AND SmartScreen reputation. **Decision**: defer Windows signing. Ship v0.1.0
without it; early Windows users click **More info → Run anyway** (documented in the release notes).
**Alternatives**: (a) **EV code-signing cert** (DigiCert/Sectigo/SSL.com, ~$250–600/yr, hardware/cloud-key)
— instant SmartScreen reputation, the "no warnings day 1" answer; (b) **Azure Trusted Signing** (~$10/mo,
cloud, Microsoft-managed) — cheap and modern but org must be 3+ yrs old or pass identity validation;
(c) **OV cert** — cheaper but reputation warms up over downloads, so early users still get warned. All
rejected *for now* on cost + weeks-long CA vetting lead time vs. an early, technical Windows audience.
**Consequences**: Windows users see a one-time SmartScreen click-through until we sign. **Revisit trigger**:
real Windows download demand → pick EV cert (instant reputation) or Azure Trusted Signing (if ZeroLogic
qualifies); the cert publisher name should match the legal entity behind bundle id `com.zerologic.*`. Build
wiring when we do it: a `tauri.windows.conf.json` (mirroring the macOS split) with
`bundle.windows.certificateThumbprint`/`signCommand` + `timestampUrl`, build the **NSIS** installer (drop
`--no-bundle`), sign exe + installer for both arm64 and x64, and verify on the Parallels Windows 11 VM with
a Mark-of-the-Web download.

### 2026-06-21: Tool registry as the single source of truth (drop the `Tool` enum)
**Status**: Approved (PRs #18 + #19). **Context**: tool knowledge was scattered — a Rust `Tool` enum + 8
hardcoded match-arm functions (label/detect/version/dests/scope/render), and on the frontend `ACCENTS`/
`ICONS_SVG`/`SHORT`/a hardcoded `SUPPORTED_TOOLS` + a `Tool` union. Adding a tool meant editing ~13 places;
adding one upstream in the CLI meant another ~13. **Decision**: a JSON registry is the single source. The Rust
`Tool` enum is removed — a tool is a `String` id; `label`/`detect`/`version`/`dests`/`scope` are registry lookups
and `render()` dispatches on a `format` key. The string id IS the serialized wire value (camelCase), so ledger
JSON stays compatible. **Alternatives**: codegen a typed union from the JSON (rejected — a generate step is "an
index to forget," the exact thing we're killing). **Consequences**: adding a tool that reuses an existing
`format` is data-only on both sides; a brand-new output shape needs one formatter function. Byte-parity tests
guard the render output. Load-bearing.

### 2026-06-21: `tools.json` is upstream-owned; installability is derived app-side
**Status**: Approved (coordinated with the `agency-agents` catalog repo). **Context**: who owns the canonical
tool list? The catalog repo already owns `divisions.json` (validated by a no-jq `check-divisions.sh`), and the
app is "a respectful frontend over the clone." **Decision**: the catalog repo owns the canonical **`tools.json`**
(twin of `divisions.json`), CI-guarded by `check-tools.sh` (the no-jq twin of `check-divisions.sh`, enforced by
`check-tools.yml`). It carries *upstream truth only* — what the CLI converts + installs. The app consumes a
bundled baseline (follow-up: refresh from the clone). Whether THIS app can install a tool is **derived, not
stored**: `installable(tool) = tool.format ∈ IMPLEMENTED_FORMATS` (the 7 formats the Rust renderer implements).
**Alternatives**: an `appRenderer`/`wired` bool in the catalog (rejected — couples upstream to app release
state; the format-membership rule is self-maintaining: ship a renderer → add its format → those tools light up).
**Consequences**: the catalog stays app-agnostic; the app's renderer coverage is one constant in two places
(`registry.rs` + `toolRegistry.ts`); the two repos share one CI-protected contract.

### 2026-06-21: Contribute transforms UPSTREAM first (Osaurus / the `skill-md` format)
**Status**: Approved. **Context**: the Rust `render/` is a byte-identical *port* of the catalog's
`scripts/convert.sh` (guarded by the parity test). A transform the app invents but `convert.sh` lacks would
**diverge** — a CLI `install.sh` user wouldn't get it. **Decision**: new transforms land in the catalog's
`convert.sh`/`install.sh` FIRST (canonical), then the app mirrors them. Osaurus shipped this way: a
`convert_osaurus` → Agent-Skills `SKILL.md` upstream, mirrored by a Rust `skill-md` format (with `slugPrefix:
"agency-"`). **Consequences**: one `skill-md` renderer covers Osaurus and any future Agent-Skills tool; the
parity test keeps the app honest. Antigravity stays app-recognized-only until upstream makes its skill
deterministic (its `date_added` is non-deterministic, so it can't share `skill-md`).

### 2026-06-23: Updater-enabled macOS release build mechanics
**Status**: Approved (v0.2.0 — the first release built WITHOUT `SKIP_UPDATER`). **Context**: turning auto-update
on exposed three latent traps that the manual-DMG (`SKIP_UPDATER`) path had always sidestepped. **Decisions**
(codified in `scripts/release.sh` + `docs/BUILD.md`, PR #22):
1. **Always pass a `--config` to the macOS build.** The tauri build-script's `macos-private-api` allowlist check
   reads **only base `tauri.conf.json`** — it does NOT honor the platform-split `tauri.macos.conf.json` where
   `macOSPrivateApi:true` lives (tauri#11142, closed "not planned"). With no `--config`, the feature (from the
   `[target.macos]` Cargo block) looks unauthorized and the build aborts. A `--config` forces a full config
   re-resolution that merges the platform file. `release.sh` always passes `--config '{"app":{"macOSPrivateApi":true}}'`.
   *Alternatives rejected*: putting `macOSPrivateApi` in base config (breaks the Linux/Windows allowlist) or the
   feature in base deps (the revert-before-cross-platform dance — fragile, must not be committed).
2. **Intel cross-compile via the rustup toolchain, not Homebrew rust.** The active `rustc` is Homebrew's
   (`/opt/homebrew/…`), host-only → `can't find crate for core` for `x86_64-apple-darwin`. Build with
   `PATH="$HOME/.rustup/toolchains/stable-aarch64-apple-darwin/bin:$PATH"` after `rustup target add x86_64-apple-darwin`.
3. **The updater signing key lives in the Keychain; store it via `$(cat keyfile)`, never a manual paste.** A
   trailing newline corrupts the stored key and signing fails with `incorrect updater private key password:
   Invalid input`. The canonical key is the file `~/.config/agency-agents-app/updater.key` (its pubkey is embedded);
   the Keychain copy must match it byte-for-byte. `release.sh` is fully Keychain-based — no `signing.env`.
**Consequences**: `release.sh` now signs updater artifacts cleanly with no manual `signer sign -f` step; the next
release "just works." **References**: `tasks/2026-06/260623_v0.2.0-ship.md`, `~/Downloads/fix-updater-keychain.sh`.

---

## 2026-09-14 — Verify a PR set together before merging any of it, and gate Windows separately

**Status**: Approved. **Context**: seven PRs were merged for v0.3.1. All seven were first merged into a throwaway
worktree off `main` and verified as one tree (279 Rust tests, `svelte-check` 322 files / 0 errors) before any of them
touched `main`. That caught nothing — but #77, verified only on macOS, broke the Windows build immediately after
merge, because `spawnSync("tauri")` resolves to a real executable on macOS/Linux and to `tauri.cmd` on Windows.

**Decision**:
1. **A combined-tree check is the minimum bar** when landing more than two PRs at once; PR-by-PR checks do not prove
   the set.
2. **A combined tree is not enough on its own** — it only exercises the host platform. Any PR touching build
   invocation, process spawning or path handling must be checked on Windows, either by `pr-check.yml` or by
   dispatching a build against the branch before merge.
3. **Never route a command through a shim or a shell when the real entry point can be called directly.** Every
   Windows failure this session was an intermediary altering or hiding the command: `tauri.cmd` (Node refuses to exec
   `.cmd`), the npm shim through PowerShell (`Unknown command: "pm"`), `-ArgumentList` stripping quotes out of inline
   JSON, and `stdio: "inherit"` swallowing the output that would have explained any of it. The fix each time was
   `node <entry>.js` — and for config, a file path rather than inline JSON.

**Consequences**: #105 resolves `@tauri-apps/cli/tauri.js` and runs it with `process.execPath`. `pr-check.yml` moves
from "nice to have" to required. **References**: PRs #77, #105; issue #87.

---

## 2026-09-14 — Windows packaging stays on CI; the ARM64 VM is a build-and-test box, not a packaging box

**Status**: Approved. **Context**: the Parallels Windows 11 ARM64 VM was provisioned to build releases locally
(VS Build Tools, SDK 10.0.26100, Rust 1.98.1 both MSVC targets, Node 22.20). It compiles both architectures natively
in about three minutes each, but neither bundler runs: WiX's `candle.exe` and NSIS's `makensis.exe` are both 32-bit
x86. `makensis` exits `0xC0000135` with every plausible cause ruled out — x86 emulation verified working,
`zlib1.dll` x86 alongside, `MSVCP60.dll` present in System32 and SysWOW64, x86 VC++ redistributable installed.

**Decision**: CI packages Windows. The VM compiles, installs CI artifacts and tests them. `bundle.targets: "all"` is
accepted as **x64-only on Windows**; any ARM64 Windows build must pass `--bundles nsis`.

**Consequences**: two gaps to document — the build requires **clang**, which the VC workload omits and CI only
satisfies because the runner image ships LLVM; and a from-scratch Windows build host cannot produce installers.
**References**: `activeContext.md` § Windows build box.


### 2026-10-28: Release v0.5.1
**Status**: Approved. **Context**: Necessary to ship `antigravity-md` support and fix multi-file ledger record consistency. **Decision**: Bump to v0.5.1. **Consequences**: Antigravity tool is now installable; multi-file schema is consistent across call sites.
**Correction (2026-10-28)**: the `antigravity-md` registration and the renderer/ledger test fixes landed in **v0.6.0**, not v0.5.1. v0.5.1 shipped the version bump (which also repaired the 0.5.0 manifest drift) and nothing else.
### 2026-10-27: In-app update check reads the latest GitHub Release
**Status**: Approved (branch feat/github-releases-updater). **Context**: app update checks used agencyagents.app/updater.json as the version source. **Decision**: the version decision now comes from GET /repos/deuriib/agency-agents-app/releases/latest; the signed latest.json attached to that release remains the install artifact; the legacy endpoint stays as a one-cycle fallback. **Consequences**: installability is detected from the asset list (View release vs Install); release automation (tag + draft release) added via release.yml; version bumps go through PRs via scripts/bump-version.mjs.

### 2026-10-28: Release v0.6.0 — opt-in automatic installs
**Status**: Approved (branch release/v0.6.0). **Context**: the "Install updates automatically" toggle persisted a flag that nothing read. **Decision**: ship v0.6.0 with the scheduler-owned auto-install flow — off by default, consent on first enable, Offline Mode as a hard gate checked before the flag, verify-then-install split with a cancel checkpoint, one atomic install slot shared with manual installs, and a relaunch that is always user-initiated. **Consequences**: version bumps must move all five manifests via `scripts/bump-version.mjs`; the v0.5.0 bump touched only `package.json`, so that release's installers reported 0.4.3 while the tag said v0.5.0, and v0.5.1 repaired the drift. Two conditions remain open: a manual smoke test on a signed build, and a native pass over the eleven corrected hint strings. **References**: `docs/release-notes/0.6.0.md`, `docs/enterprise-feature-auto-install-phase4.md`.

### 2026-11-01: Antigravity per-role tool declarations (issue #26)
**Status**: Approved (branch 26-featrender-port-per-role-antigravity-tools-list). **Context**: #22 shipped the Antigravity role flag but not the role's tool surface, so every installed Antigravity agent inherited the harness default instead of a declared, least-privilege set — and the orchestrator never declared its delegation tools. **Decision**: port upstream `AntigravityRenderer` verbatim — `MAIN_AGENT_TOOLS` (11: the four delegation tools plus the seven execution tools) for a primary agent, `SUBAGENT_TOOLS` (7: execution only) for everyone else, emitted as an unquoted YAML flow list after the role flag and **replacing** any source `tools:` frontmatter, because Claude-style names match nothing in Antigravity. **Consequences**: every Antigravity render changes bytes, so the whole Antigravity scope flips to `Outdated` once on the next check (one-time, user-visible); `convert.sh` parity is untouched (the gate does not cover `antigravity-md`). Upstream's `version`/`vibe`/`color` fields remain unported. **Evidence**: an independent run of the upstream converter (290 agents) matched the app's `tools:` and role-flag lines on all 274 shared agents, 0 divergence. **References**: issue #26, `src-tauri/src/render/mod.rs`, upstream `converter.py#AntigravityRenderer`.

### 2026-10-31: Primary-agent render policy (issue #22)
**Status**: Approved (branch feat/22-orchestrator-primary-mode). **Context**: the orchestrator leads a session, so tools that distinguish a primary agent from subagents must expose it as the main agent; specialists stay delegatable subagents. The vendored `scripts/convert.sh` predates this policy and hardcodes `mode: subagent` / `subagent: true` for every agent. **Decision**: the renderer mirrors upstream `agency_agents/core/converter.py#PRIMARY_AGENT_SLUGS` — keyed on the slugified frontmatter `name`, currently only `agents-orchestrator`. OpenCode emits `mode: primary` for a primary agent (specialists keep `mode: subagent`); Antigravity emits `mainAgent: true` and drops `subagent: true` (the flags are mutually exclusive), while specialists keep `subagent: true`. The `convert.sh` byte-parity contract remains for all other output; the `--ignored` parity gate normalizes the single OpenCode `mode:` line for the primary agent. **Consequences**: only the orchestrator's OpenCode/Antigravity renders change (at most two ledger rows flip to `Outdated`/`Substantive`); specialist output stays byte-identical. Upstream's per-role Antigravity `tools:` list is not ported, and the runtime tie-break when `mainAgent` is omitted is upstream behavior — both are follow-ups. **References**: issue #22, `src-tauri/src/render/mod.rs`, upstream `converter.py`.

### 2026-11-01: The tool registry is schema-validated app-side (issue #30)
**Status**: Approved (branch 30-feat-tools-json-schema). **Context**: `tools.json` is upstream-owned and consumed by two parsers (Rust `registry.rs`, frontend `toolRegistry.ts`), but nothing checked an entry's shape before runtime — a typo in `accent`/`format`/`dest` reached both consumers silently. The issue asked for a `_schema.json` in `integrations/`, a directory that only exists in the catalog repo for generated artifacts; this repo's manifest lives at `src-tauri/data/tools.json`. **Decision**: add `src-tauri/data/_schema.json` (Draft 2020-12) beside the mirror and validate it in `src-tauri/tests/tools_schema.rs` (`jsonschema` dev-dependency, `default-features = false`). The manifest itself stays byte-identical — no `$schema` key, because every sync from upstream would drop it; `.vscode/settings.json` provides the IDE mapping instead, and the schema *allows* `$schema` so the canonical manifest can adopt it. `order` is optional because both consumers already treat it as optional. **Consequences**: editing the manifest now fails `cargo test --test tools_schema` on a missing required field, a malformed value/enum, or an unknown key (negative cases are asserted too, so the schema cannot rot into a no-op). No CI job runs the Rust suite yet, so the gate is local and documented as such. **References**: issue #30, `src-tauri/data/_schema.json`, `src-tauri/tests/tools_schema.rs`.

### 2026-11-01: Repository automation — release-please, PR gates, declarative labels, mise

**Status**: Approved (branch chore/repo-professionalization). **Context**: releases were cut by hand (`scripts/bump-version.mjs` → version-bump PR → `release.yml` tag + draft release gated on `docs/release-notes/<v>.md`), no workflow ran on pull requests, labels were edited in the GitHub UI, and Dependabot only watched Actions. The repo had no issue templates, PR template, CODEOWNERS, or secret scan despite a `.gitleaks.toml` sitting unused. **Decision**: (1) `release-please` owns versioning — Conventional Commits on `main` accumulate into a release PR that bumps `package.json`, `package-lock.json`, `src-tauri/tauri.conf.json` (json extra-file) and `src-tauri/Cargo.toml` (toml extra-file); merging it creates the `vX.Y.Z` tag and a **draft** release, preserving the existing flow where Linux/Windows CI attaches bundles and macOS is signed manually via `scripts/release.sh` before a human publishes. `draft: true` + `force-tag-creation: true`; `bump-minor-pre-major` so breaking changes move 0.x minor, not to 1.0.0. Optionally appends `docs/release-notes/<v>.md` below the generated changelog. `release.yml` is deleted; `linux-build.yml`/`windows-build.yml` are now `workflow_call` only. (2) `pr-check.yml` gates every PR on `cargo fmt --check`, `cargo clippy -D warnings`, the Rust suite, `svelte-check`, the frontend build, and a Conventional-Commit PR title. (3) Labels are declarative (`.github/labels.yml` + `sync-labels.yml`) with path-based auto-labelling. (4) `.mise.toml` + `mise.lock` pin the toolchain and expose the CI commands as tasks (`mise run ci`). **Consequences**: the repo-wide `cargo fmt` pass in this branch is what makes the fmt gate green; release PRs opened with the default `GITHUB_TOKEN` start their PR checks in an approval-required state (a maintainer clicks "Approve workflows to run" in the merge box; no PAT needed); the repository setting "Allow GitHub Actions to create and approve pull requests" is required and was enabled; `src-tauri/Cargo.lock` stays derived (the next cargo command refreshes the local package entry); `scripts/bump-version.mjs` remains only as the manual fallback. **References**: `release-please-config.json`, `.release-please-manifest.json`, `.github/workflows/release-please.yml`, `.github/workflows/pr-check.yml`, `.github/labels.yml`, `.mise.toml`, `mise.lock`.

---

### 2026-11-02: The updater manifest is a release asset; channels select a release stream

**Status**: Approved (merged as `a453bd2`, PR #38, 2026-11-02). **Context**: the in-app updater could never install
anything. No release ever carried `latest.json` (404), Linux/Windows explicitly disabled
updater artifacts, the fallback host served a v0.3.2 manifest for the upstream owner, and
releases stayed drafts. The version decision also used a different source (GitHub
`/releases/latest`) than the installer (a static manifest endpoint), which made channels
impossible to add safely.

**Decision**:
1. **One manifest per release, attached to that release.** `scripts/updater-manifest.mjs`
   builds/merges it from the platform artifacts and their `.sig` files; CI builds the
   Linux/Windows entries, `scripts/release.sh` merges the `darwin-*` entries and
   publishes. Stable clients fetch `releases/latest/download/latest.json`.
2. **Channels select a release stream.** `Settings::update_channel` (stable | beta) is
   persisted. Stable uses `/releases/latest`; beta uses the newest published prerelease
   from `/releases?per_page=30`, falling back to stable. The manifest URL is resolved per
   check and cached with the announced version, so a channel change between check and
   install cannot download a different release's bytes. Beta installs from the rolling
   `updater-beta` release; that release must never be "latest".
3. **The draft-then-human-publish model stays.** CI attaches assets but never publishes;
   macOS notarization is local, so `release.sh` is the single publish step and refuses to
   publish a manifest missing Linux or Windows.
4. **The release payload must match the shipped bundle.** The AppImage is repacked after
   Tauri signs its updater tarball, so the Linux job deletes the pre-repack tarball and
   regenerates + re-signs one from the repacked AppImage.

**Alternatives rejected**:
- Keeping the `agencyagents.app/updater.json` host: it served a stale v0.3.2 manifest for
  the upstream owner, and it sits outside the release pipeline.
- Adopting `tauri-action`: it would replace the release-please + draft + local macOS flow
  the repo already uses, including the `docs/release-notes/` step.
- A single manifest for both channels: Tauri's platform keys have no channel dimension,
  so a client cannot tell a beta entry from a stable one.
- Auto-publishing from CI: a draft whose macOS assets are still being notarized would
  serve a manifest pointing at files no client can download.

**Consequences**: a release is not done until `latest.json` is attached and reachable by
an anonymous client (`mise run verify-updater`). The two signing secrets are required for
Linux/Windows builds; a missing key fails the preflight. The v0.7.1 draft was stranded
before this change and should be deleted. The real install remains unverified until the
first signed, published release.

**References**: `scripts/updater-manifest.mjs`, `scripts/verify-updater-manifest.mjs`,
`src-tauri/src/commands/updater.rs`, `src-tauri/src/github/releases.rs`,
`.github/workflows/release-please.yml`, `scripts/release.sh`,
`docs/updater-release.md`, `tasks/2026-11/261102_updater-pipeline.md`.

### 2026-11-04: A build that cannot self-update must not offer it

**Status**: Approved (merged as `3ba967d`, PR #102, 2026-11-04).
**Context**: `.deb` and `.rpm` installs failed *every* update with `invalid updater binary
format`, after the full ~85 MB had downloaded, and the failure was reported as a network
error. Nothing about it was recoverable: two facts in the toolchain disagree and neither is
ours to change.

1. `tauri-utils` bakes a bundle-type sentinel into the binary, and `tauri-plugin-updater`
   picks its installer from it — so a package install always calls `install_deb` /
   `install_rpm`, which validate the payload and reject anything that is not a real
   package.
2. `tauri_plugin_updater::target()` builds its platform key as `{os}-{arch}` with no
   installer dimension, so `platforms` can carry exactly **one** artifact per platform.
   Ours is the AppImage tarball, because the AppImage is the artifact that self-updates
   correctly today.

**Decision**:
1. **Read the bundle type and refuse where there is no path.**
   `blocked_package_format(Option<BundleType>)` is a pure gate: `Deb` / `Rpm` blocked,
   AppImage / `App` / `Dmg` / `Msi` / `Nsis` / `None` (a `tauri dev` binary, which carries
   no sentinel) unchanged. No runtime, no network, no built artifact needed to test it.
2. **The capability travels in the payload, not inferred by the UI.**
   `Available` gained `inAppInstall` (a property of the *build*) alongside the existing
   `installable` (a property of the *release*). They are never merged: a release can carry
   a perfectly good manifest that a given build still cannot consume, and collapsing the
   two is precisely what produced the bug.
3. **Refuse before the download, on both paths.** `run_install` gates ahead of
   `download_and_install` via `self_update_gate`, and `auto_install_blocker` returns
   `UnsupportedBundle` so the scheduler stops emitting a `Started`/`Failed` pair on every
   wake.
4. **Typed errors, never `Network`.** `InvalidUpdaterFormat` (the plugin rejected the bytes)
   and `SelfUpdateUnsupported` (the pre-flight gate) are their own variants. A local,
   deterministic dead end must not tell the user to check their connection.
5. **Offer the release's own installer.** `package_download` carries the matching `.deb` /
   `.rpm` URL; the card deep-links it and says why in one sentence. No asset → degrade to
   "View release", never to a broken link.

**Alternatives rejected**:
- **Point `linux-x86_64` at the `.deb`.** Fixes package installs, breaks AppImage
  self-update — trading the working path for the broken one.
- **Ship Linux as AppImage only.** Drops `.deb` / `.rpm` from the distribution story and
  ignores that users already installed them.
- **A privileged custom updater.** Duplicating `pkexec` / polkit elevation is a bigger,
  riskier surface than the plugin's own implementation.
- **Document "Linux auto-update is AppImage-only" and keep the button.** A documented dead
  end that still pays 85 MB per click is not a fix.

**Consequences**: a `.deb` / `.rpm` build shows "Download .deb" instead of "Install
update", and its auto-install toggle is disabled once a check proves it cannot work. This
is a **permanent constraint**, not a bug to re-report: making Linux package installs
self-updatable needs per-installer manifests (`latest-deb.json`, `latest-rpm.json`) **and**
a signing step for `.deb` / `.rpm`, which carry no `.sig` today (#56 option B). The app
already re-resolves its endpoint per install, so the plumbing exists; the signing does not.

**References**: `src-tauri/src/commands/updater.rs` (`blocked_package_format`,
`package_download_for`, `self_update_gate`, `auto_install_blocker_for_bundle`),
`src-tauri/src/github/releases.rs` (`PackageFormat`, `PackageAsset`, `package_assets`),
`src-tauri/src/error.rs`, `docs/updater-release.md#linux-package-installs-are-download-only-56`,
`tasks/2026-11/261104_updater-package-installs.md`.

### 2026-11-04: Release notes are flattened at the source, before they are truncated

**Status**: Approved (merged as `3ba967d`, PR #102, 2026-11-04).
**Context**: the Updates card rendered `release-please`'s generated Markdown as literal
text — `## [0.8.3](https://…)`, list markers, bare URLs. Two comments asserted the
opposite of the code, which is why it survived review: the renderer was right when `notes`
was one short sentence, and the field grew into the whole release body without it.

**Decision**:
1. **Flatten in the backend, at the source.** `github::releases::strip_markdown` reduces
   headings, bullets, thematic breaks, inline links, `**bold**` / `__bold__` / `*italic*`
   and code spans to plain text. Every consumer gets clean text; the card keeps
   interpolating `{notes}` and stays auto-escaped.
2. **No Markdown renderer, no HTML, no sanitizer.** A dependency plus an XSS surface is not
   worth it for a card whose full content is one click away, and
   `dangerouslySetInnerHTML` is barred without DOMPurify. A hand-written scanner emits
   text, so the escaping story is unchanged.
3. **Flatten before bounding.** `bounded_notes` strips first, then applies
   `MAX_NOTES_CHARS`. Truncating first can cut a link or a URL in half; this order makes
   the whole class unrepresentable rather than something to guard against at the edge.
4. **Underscores need word boundaries.** Emphasis delimiters follow CommonMark flanking
   rules: `_` only opens at a word boundary and only closes before one. `IMPLEMENTED_FORMATS`
   and `snake_case` are the most common identifiers in these notes, and blind stripping
   mangled them. Whitespace-wrapped markers (`a * b * c`) stay literal for the same reason.
5. **Tolerate malformed input.** Unbalanced syntax (`[text` with no `]`, an unclosed code
   span, `[label]` with no target) passes through unchanged. A hostile or truncated body
   must never swallow the rest of the text.

**Alternatives rejected**:
- **Render a Markdown subset properly.** Library + sanitizer + an XSS surface for a
  Settings card.
- **Shorten at the source instead** (first paragraph only). Cheaper, but discards the
  per-release bullets that are actually useful in the card.
- **Strip in the frontend.** The truncation lives in the backend, so a frontend strip
  cannot repair a link already cut at the bound.

**Consequences**: `notes` is plain text by contract, and `types.ts` says so.
`MAX_NOTES_CHARS` now bounds *visible* text rather than raw Markdown, so a release body
shows more of itself than before at the same bound. The stripper is deliberately not a
Markdown parser: a future construct (tables, images, footnotes) will pass through and need
a decision, not a bug report.

**References**: `src-tauri/src/github/releases.rs` (`strip_markdown`, `strip_line_prefix`,
`strip_inline`, `match_link`, `bounded_notes`), `src/lib/types.ts`, `src/lib/components/SettingsSectionUpdates.svelte`,
`tasks/2026-11/261104_updater-package-installs.md`.
