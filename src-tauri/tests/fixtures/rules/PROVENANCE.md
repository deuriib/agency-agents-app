# Rules conformance fixtures — provenance

These fixtures are **generated output**, not hand-written. They are the oracle
the native Rust renderers are compared against byte-for-byte
(`tests/rules_conformance.rs`).

## Source

| Field | Value |
| --- | --- |
| Repository | `deuriib/agency-rules` |
| Checkout used | `/home/deuriib/projects/agency-rules` |
| Commit | `bde758c3f6877e3f75a26eda345051a6cdba2860` |
| Generated on | 2026-11-03 |
| Rules | 8 sources (`rules/base/*.md`, `rules/stacks/*.md`) |
| Comparisons | 8 rules × 4 harnesses = **32** |

## Regeneration command

Run from the `agency-rules` checkout. `INTEGRATIONS_DIR` redirects the compiler
to a scratch directory, so the checkout's own `integrations/` tree is never
touched:

```sh
cd /home/deuriib/projects/agency-rules
mkdir -p /tmp/rules-oracle
INTEGRATIONS_DIR=/tmp/rules-oracle node bin/agency-rules.mjs convert
```

Then vendor the artifacts verbatim (no editing, no re-formatting):

```sh
FIXTURES=/home/deuriib/projects/agency-agents-app/src-tauri/tests/fixtures/rules
for h in antigravity pi-agent oh-my-pi opencode; do
  (cd "/tmp/rules-oracle/$h" && find . -type f ! -name '_manifest.json' \
     -exec cp --parents {} "$FIXTURES/$h/" \;)
done
cp /home/deuriib/projects/agency-rules/rules/base/*.md \
   /home/deuriib/projects/agency-rules/rules/stacks/*.md \
   "$FIXTURES/sources/"
```

`_manifest.json` files are deliberately **not** vendored: M1 never reads them
(`docs/rules-integration-tasks.md`, out-of-scope list), and they carry a
timestamp-free but redundant inventory.

## Why the fixtures are regenerated instead of copied from `integrations/`

Ruling **C5 (revised)**. The checked-in `agency-rules/integrations/` tree is
stale: it was produced at `1c846f5`, `agentmemory` drifted in `82befdc` and
`bde758c`, and `mcp-gateway-protocol` compiles but was never regenerated there.
Copying it would fail four of the 32 assertions against a *correct* renderer.

The upstream `integrations/` directory is generated output and is gitignored
upstream. It must never be treated as a checked-in oracle.

## `block-scalars.json`

`block-scalars.json` is a second oracle, for the YAML emitter's block-scalar
branch (issue #63). Each case is `{ name, value, expected }`, where `expected`
is exactly what the reference produces for that `description`:

```sh
node --input-type=module -e "
import YAML from 'yaml';
process.stdout.write(JSON.stringify(YAML.stringify({ description: VALUE }).trimEnd()));
"
```

(`stringify(...).trimEnd()` is what `scripts/lib/renderers/_util.mjs` does, so
the oracle is the same pipeline the compiler uses, not a re-derivation.)

The matrix covers every chomping indicator, the `2` indent indicator, leading and
trailing whitespace, blank and more-indented lines, unicode, a carriage return
and the folded shapes the emitter still refuses. Cases whose `expected` starts
with `description: >` assert the **blocker**, not bytes: `>` folding is the one
branch not ported, and the test fails if the guard stops catching it.

Do not hand-edit the file; regenerate it when the matrix changes.

## Staleness defence

CI cannot run the Node compiler, so this file is the only record of where the
bytes came from. Two guards keep the fixtures honest:

1. `tests/rules_conformance.rs` fails on any byte drift between the renderers
   and these files.
2. The `#[ignore]`d `rules_parity_against_checkout` test re-renders from a live
   `AGENCY_RULES_PARITY_ROOT` checkout and diffs its `integrations/`. A rule
   that exists upstream with no compiled artifact is **reported**, not failed
   (a stale upstream tree must not be mistaken for a renderer bug).

When `agency-rules` changes, regenerate with the command above, re-vendor, and
update this file's commit SHA in the same commit.
