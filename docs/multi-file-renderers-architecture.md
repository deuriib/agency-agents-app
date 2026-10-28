# Multi-File Renderer Architecture Proposal

> Status: **Proposal** — not yet implemented. Companion to `render/mod.rs`,
> `install/mod.rs`, and `types.rs`. Targets the backlog items Aider (roster),
> Windsurf (roster), Kimi (per-agent multi-file), OpenClaw (per-agent
> multi-file), and Vibe (toml + prompt md). Antigravity already uses the
> single-file `gemini-md` renderer and serves as the precedent.

## 1. Problem statement

Today `render::render` returns a single `String` and `render::dests` returns
`Vec<PathBuf>` (used for dual-write tools such as Copilot). The install core
(`install::write_agent_files_to`) assumes **one rendered payload written to
every dest**:

```rust
// install/mod.rs:448
let (bytes, rendered_hash) = render::render_with_hash(agent, raw, tool)?;
...
for dest in &paths { atomic_write(dest, bytes.as_bytes()).await?; }
```

That invariant breaks for the five backlog tools:

- **Kimi** — `agent.yaml` + `system.md` (two different payloads).
- **OpenClaw** — `SOUL.md` + `AGENTS.md` + `IDENTITY.md`.
- **Vibe** — `.toml` + `.md`.
- **Aider / Windsurf** — one *accumulated* file (roster) aggregating every
  installed agent.

The ledger (`InstallRecord.dest: String`, `rendered_hash: String`) and the
reconcile loop (`installs_reconcile`) are built around "one file per
logical install", so the design must extend them without losing the
deterministic render → hash → reconcile contract (`contracts.md` §E,
`systemPatterns` §4).

## 2. Decision 1 — `render()` signature

**Recommendation: add `render_files()`; keep `render()` as a thin shim.**

```rust
// render/mod.rs (new)
pub struct RenderedFile {
    /// Path *suffix* relative to the tool's agents root, e.g. "agent.yaml",
    /// "system.md", "SOUL.md". Joined with dests() templates.
    pub relative: PathBuf,
    /// Deterministic UTF-8 payload.
    pub bytes: String,
}

pub fn render_files(
    agent: &Agent, raw_source: &str, tool: &str,
) -> Result<Vec<RenderedFile>, AppError>;

/// Back-compat shim. Returns the single payload for single-file tools;
/// errors for multi-file tools (preserves today's "unsupported" gate).
pub fn render(...) -> Result<String, AppError> {
    let files = render_files(...)?;
    if files.len() != 1 { return Err(unsupported(tool)); }
    Ok(files[0].bytes.clone())
}
```

**Why not change `render()` to `Vec` outright?** Blast radius: ~30 call
sites in `install/mod.rs` (write path, `track_agent_record`,
`remove_agent_files`, `bytes_match_render`, reconcile's re-render, `agent_diff`)
plus the parity tests in `render/mod.rs`. A shim lets multi-file tools land
behind a new code path while single-file tools and their tests stay untouched.
Once all five tools ship, `render()` can be deleted and call sites migrated in
one PR.

**Determinism is preserved**: `render_files` is still a pure function of
`(Agent, raw_source, tool)`; the registry's `format` key gains new variants
(`kimi-multi`, `openclaw-multi`, `vibe-multi`, `aider-roster`,
`windsurf-roster`) whose templates are deterministic heredocs, mirroring
`convert.sh`.

## 3. Decision 2 — hashing multi-file installs

**Recommendation: composite hash + per-file hashes.**

- `rendered_hash` (ledger column, unchanged type) becomes the SHA-256 of a
  **canonical framing** of all files:

  ```
  composite = SHA-256( for each file in sorted-by-relative-path order:
                         u32le(relative.len()) || relative bytes
                         || u64le(content.len()) || content bytes )
  ```

  Length-prefixing prevents `a/b` + `c` colliding with `a/` + `bc`. This keeps
  the existing single-column reconcile contract (`classify(disk_hash, rendered_hash, ...)`)
  working: the on-disk composite is computed by reading every dest and framing
  the same way.

- Add to `InstallRecord` (serde-defaulted for migration):

  ```rust
  #[serde(default)]
  pub file_hashes: BTreeMap<String, String>, // relative path -> sha256
  ```

  `file_hashes` is what makes "which file changed?" answerable and lets
  `agent_diff` show per-file diffs. It is **derived**, never authoritative;
  `rendered_hash` stays the reconcile key.

**Alternative considered: per-file ledger rows.** Rejected — it explodes the
"one logical install" model (`installs_reconcile` collapses by
`(slug, tool, project_path)`; `projects_list` counts rows; foreign sweep
adopts rows). A composite keeps the cardinality invariant.

## 4. Decision 3 — roster (accumulated) tools

Aider and Windsurf do not have per-agent files. `CONVENTIONS.md` /
`.windsurfrules` is one file that aggregates *all* installed agents. This
conflicts with per-slug reconciliation.

**Recommendation: treat roster tools as an "aggregate container".**

- Install key changes from `(slug, tool, project)` to `(tool, project)` for
  roster tools; the slug list lives in a new ledger field:

  ```rust
  #[serde(default)]
  pub aggregate_slugs: Vec<String>, // only meaningful for roster tools
  ```

- Writing: `do_install` for a roster tool re-renders **every** slug currently
  in the aggregate (the new one plus existing rows for the same
  `(tool, project)`), concatenates their blocks with a stable separator, and
  writes the single roster file. `rendered_hash` = hash of the whole file.

- Reconcile: group ledger rows by `(tool, project_path)` for roster tools;
  recompute the expected aggregate from the union of `aggregate_slugs` and
  compare to on-disk hash. If equal → `Current` for all rows in the group;
  if any slug's source advanced → `Outdated`; if bytes differ → `Modified`.

- Uninstall of one slug re-renders the aggregate *without* that slug (or
  deletes the file + all rows if it was the last).

**Trade-off**: roster installs are O(n) in the number of installed agents per
write, and concurrent installs must serialize on a per-`(tool, project)` lock.
Acceptable — roster files are small and installs are user-driven, not
high-throughput.

**Alternative considered: per-agent managed blocks with markers.** Rejected
for v1: reconcile would need to parse markers, and partial edits by the user
become ambiguous. Full re-render is deterministic and matches how
`convert.sh` produces these files.

## 5. Decision 4 — ledger schema migration

`InstallRecord` (types.rs:199) gains, all `#[serde(default)]`:

```rust
pub struct InstallRecord {
    // ... existing fields ...
    pub dest: String,                       // primary/first path (kept for compat)
    #[serde(default)]
    pub dests: Vec<String>,                 // NEW: all physical paths
    #[serde(default)]
    pub file_hashes: BTreeMap<String, String>, // NEW: relative -> sha256
    #[serde(default)]
    pub aggregate_slugs: Vec<String>,       // NEW: roster-only
}
```

Migration rules (load-time, in `load_ledger`):

1. If `dests` is empty → `dests = vec![dest]`.
2. If a row is a roster tool and `aggregate_slugs` is empty → treat as
   single-slug aggregate `[slug]` (so old single installs migrate).
3. No data loss: old `dest`/`rendered_hash` remain valid for single-file
   tools; reconcile falls back to `dests[0]` when `file_hashes` is empty by
   hashing that one file.

`InstalledAgent.dest` (types.rs:248) stays a single `String` for the UI
(primary path); multi-file dests are exposed via a new optional
`dests: Vec<String>` (serde-default) when the frontend needs them.

## 6. Decision 5 — partial writes

`write_agent_files_to` (install/mod.rs:435) must become transactional:

1. Render all files first (pure, before any I/O). If render fails, nothing
   touched.
2. For each dest: `backup_if_differs` (existing, install/mod.rs:154) — already
   preserves prior bytes.
3. Write to a temp file per dest (`<dest>.tmp.<pid>`), `fsync`, then
   `rename` (atomic on same filesystem). `atomic_write` already does this;
   reuse it per file.
4. Track written paths in a `Vec`. On any error mid-way, run a rollback pass:
   restore each already-written dest from its `.bak` (or delete if it did not
   exist before), then return the error. Ledger is **not** saved until all
   writes succeed.
5. Save ledger (`save_ledger`) only after all renames commit.

Roster tools additionally acquire a per-`(tool, project)` async mutex so two
concurrent installs cannot interleave roster rewrites.

## 7. Reconcile changes

`installs_reconcile` (install/mod.rs:711) needs three edits:

- **Per-file composite**: for each ledger row, read every `dests` path, frame
  them, hash, compare to `rendered_hash`. Missing any → `Removed`. Any
  mismatch → `Modified`.
- **Roster groups**: for roster tools, group rows by `(tool, project_path)`,
  recompute expected composite from the union of `aggregate_slugs`, compare
  once, and apply the resulting state to every row in the group.
- **Foreign sweep** (`agent_units`, install/mod.rs:933): roster tools have no
  per-slug unit to scan, so skip them in the foreign sweep (they are never
  "foreign single files"). Per-agent multi-file tools add their *leaf* suffix
  (e.g. Kimi's `system.md`) to `agent_units` so the sweep can recognize them;
  a dir is recognized only when *all* expected leaves exist and at least one
  matches a render.

`bytes_match_render` (install/mod.rs:525) and `agent_diff` (install/mod.rs:605)
must call `render_files` and compare the framed composite; `agent_diff` gains a
per-file view (optional, frontend-gated).

## 8. Minimal migration plan

Phase 0 — schema (no behavior change):
1. Add `dests`, `file_hashes`, `aggregate_slugs` to `InstallRecord` with
   `#[serde(default)]` (types.rs:199). Add `dests` to `InstalledAgent`.
2. Update `load_ledger` to backfill `dests` from `dest`.
3. Update `record_for` (install/mod.rs:124) to populate `dests`.

Phase 1 — render API:
4. Add `RenderedFile` + `render_files()` + shim `render()` (render/mod.rs).
5. Add registry format variants + dest templates for kimi/openclaw/vibe.
6. Implement renderers for kimi, openclaw, vibe (per-agent multi-file).

Phase 2 — install core:
7. Rewrite `write_agent_files_to` to iterate `render_files()` × `dests()`,
   pairing by index/order, transactional with rollback.
8. Update `remove_agent_files`, `track_agent_record`, `candidate_dests`.

Phase 3 — roster:
9. Add aggregate mutex + roster renderer; implement aider, windsurf.
10. Grouped reconcile + uninstall re-render.

Phase 4 — reconcile + diff:
11. Composite hash in `classify` path; per-file `file_hashes`.
12. Foreign sweep leaves for per-agent multi-file tools; skip roster.

Phase 5 — tests + docs:
13. Parity tests (ignored, `AGENCY_AGENTS_PARITY_ROOT`).
14. Update `contracts.md` §B matrix and `systemPatterns`.
15. TS types in `src/lib/types.ts` (dests, fileHashes).

## 9. Trade-offs summary

| Decision | Chosen | Alternative | Why |
|---|---|---|---|
| render API | add `render_files`, keep `render` shim | change `render` to `Vec` | Smaller blast radius; incremental landing |
| hash | composite + per-file map | per-file ledger rows | Preserves one-row-per-logical-install invariant |
| roster | full re-render aggregate | marker blocks | Deterministic, matches convert.sh, simpler reconcile |
| migration | serde-default backfill at load | one-shot migration command | Zero user action, old ledgers keep working |
| partial writes | temp + rename + rollback | best-effort writes | Never leave a half-installed multi-file set |

## 10. Citations

- `src-tauri/src/render/mod.rs:236` `render` (single String)
- `src-tauri/src/render/mod.rs:355` `dests` (Vec<PathBuf)
- `src-tauri/src/render/mod.rs:340` `render_with_hash`
- `src-tauri/src/install/mod.rs:124` `record_for` (`dest` single)
- `src-tauri/src/install/mod.rs:435` `write_agent_files_to` (one payload × N dests)
- `src-tauri/src/install/mod.rs:492` `classify`
- `src-tauri/src/install/mod.rs:525` `bytes_match_render`
- `src-tauri/src/install/mod.rs:711` `installs_reconcile`
- `src-tauri/src/install/mod.rs:933` `agent_units`
- `src-tauri/src/types.rs:199` `InstallRecord`
- `src-tauri/src/types.rs:243` `InstalledAgent`
- `memory-bank/contracts.md` §B (install-target matrix), §E (determinism)
