# 261101_tools-json-schema

## Objective
Resolve issue #30: give the bundled `tools.json` a JSON Schema so malformed entries fail at
authoring time (IDE + test) instead of at runtime, in either consumer.

## Outcome
- ✅ `src-tauri/data/_schema.json` — Draft 2020-12 schema for the manifest: root `_note`/`tools`,
  `$defs.tool` with 11 required fields, `additionalProperties: false`, enums (`installKind`,
  `slugFrom`), patterns (`id` camelCase, `kebab`, `accent` hex, `format`, `slugPrefix`), a
  roster⇒`slugFrom: null` rule, and `dest`/`detect.dirs` guards against absolute or `..` paths.
- ✅ Every property carries a `description` (15/15 tool properties), so the schema doubles as
  the field documentation editors show.
- ✅ `.vscode/settings.json` maps `/src-tauri/data/tools.json` → `./src-tauri/data/_schema.json`,
  so VS Code validates and autocompletes while the manifest is edited.
- ✅ `src-tauri/tests/tools_schema.rs` enforces the contract: the real manifest must validate,
  and the schema must still reject a missing required field, a malformed value/enum, and an
  unknown key. A schema that accepts everything cannot pass this suite.
- ✅ `cargo test`: 355 passed, 7 ignored (baseline was 350/1; the +5 are these tests).
- ✅ Independent EvidenceQA PASS, including a run through VS Code's real JSON language service:
  0 diagnostics on `tools.json`, tooltips and completion working.

## Files Modified
- `src-tauri/data/_schema.json` — NEW: the schema (Draft 2020-12, `$id` for the app mirror).
- `src-tauri/tests/tools_schema.rs` — NEW: positive + negative validation tests.
- `src-tauri/Cargo.toml` — `jsonschema 0.60` as a **dev-dependency**, `default-features = false`
  (no blocking HTTP client / TLS stack; the schema uses only same-document `#/$defs` refs).
- `src-tauri/Cargo.lock` — jsonschema chain (+21 crates) plus unavoidable patch re-resolutions
  (`num-bigint`, `regex-automata`, `zmij`, `winapi-util`→`windows-sys 0.61`).
- `.vscode/settings.json` — `json.schemas` mapping.
- `CONTRIBUTING.md`, `README.md` — document the schema and the local test command.

## Patterns Applied
- Upstream ownership: `src-tauri/data/tools.json` is untouched (byte-identical to `main`); the
  schema describes it rather than changing it.
- The Rust registry remains the runtime parser (`registry.rs` `include_str!`); the schema is a
  shape gate, not a second parser.
- Tests that can fail: negative cases are first-class, mirroring the repo's "evidence, not vibes"
  stance for renderer parity.

## Integration Points
- `src-tauri/src/registry.rs` / `src/lib/data/toolRegistry.ts` — both consume the same file the
  schema validates; no production code changed.
- CI: no workflow runs the Rust suite yet (pre-existing gap), so this is a local gate documented
  in CONTRIBUTING, not a CI gate.

## Architectural Decisions
- Schema lives next to the manifest it validates (`src-tauri/data/_schema.json`), not in a
  non-existent `integrations/` directory (the issue's path came from the catalog's generated
  artifacts tree).
- The app mirror intentionally carries no `$schema` key (it would make every upstream sync
  noisy); the schema *allows* one so the canonical manifest can adopt it later.
- `order` is optional in the schema because both consumers treat it as optional
  (`Option<u32>` + `unwrap_or(999)`; `order?: number`).

## Notes
- `installKind: "roster"` with `slugFrom` **omitted** is still accepted (the rule only rejects a
  non-null value). Consumers treat absent as null; documented, low risk.
- Upstream follow-up: mirror `_schema.json` into the canonical catalog next to its `tools.json`,
  add `$schema` there, and (optionally) add a CI job that runs `cargo test --test tools_schema`.
