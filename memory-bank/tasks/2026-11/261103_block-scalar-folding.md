# 261103_block-scalar-folding

## Objective

Issue #63. Rule installation refused any rule whose frontmatter carried a
newline (`render::render_blockers`, ruling C11), because the emitter only wrote
single-line scalars. A schema-valid rule can carry a multi-line `description`, so
that was a real gap rather than a theoretical one.

## Outcome

- ✅ **Multi-line values are installable.** The literal block, all three chomping
  indicators (`-` strip, clip, `+` keep), the `2` indent indicator for a value
  starting with a space, and both quoted fallbacks (trailing whitespace after a
  newline; a carriage return) are ported from `yaml@2`'s `blockString` and proven
  byte-identical against the reference compiler.
- ✅ **A generated oracle, not hand-written expectations**:
  `tests/fixtures/rules/block-scalars.json`, 28 cases produced by
  `yaml.stringify(...).trimEnd()` — the exact pipeline `_util.mjs` uses.
  `block_scalars_match_the_reference_compiler` walks them: 24 must match byte for
  byte, 4 folded shapes must be refused.
- ✅ **The guard is narrowed, not deleted.** `render_blockers` now fires on ONE
  shape: a value with a line longer than `lineWidth - indent` (78 columns), which
  `yaml@2` folds into `>`. Its folding rules — blank-line doubling, more-indented
  blocks, the overflow fallback to literal — are not ported, and writing those
  bytes would break the module's `rendered_hash` contract.
- ✅ Two bugs fixed on the way, both caught by the oracle: `indent_newline_runs`
  dropped the indent after a newline run at end-of-string (the `leading newline`
  case), and `double_quoted` emitted `\x0a` where the reference writes `\n`.
- ✅ `emit_block` now mirrors the reference's `.trimEnd()`, which is load-bearing
  for `|+` (the kept trailing newlines are then dropped).

## What is left (deliberately)

The folded branch (`>`). The issue's own rule is "remove `render_blockers` only
when the golden fixtures cover the shapes the guard currently refuses", so the
honest increment is: cover the literal shapes, refuse the folded one with a
message that names it. Porting `>` folding removes the remainder.

## Files

**New**

- `src-tauri/tests/fixtures/rules/block-scalars.json` (generated)

**Modified**

- `src-tauri/src/rules/render.rs` — `block_scalar`, `indent_newline_runs`,
  `ends_with_newline_then_whitespace`, `line_length_over_limit`,
  `indent_isolated_newline_runs`, `double_quoted`, `emit_block`, a narrowed
  `render_blockers`, and the oracle test
- `src-tauri/src/rules/commands.rs` — the multi-line install test inverted (it
  installs now) plus a residual-guard test
- `src-tauri/tests/fixtures/rules/PROVENANCE.md` — how to regenerate the oracle
- `docs/rules-integration-tasks.md` — ruling C11 updated to the narrowed shape

## Patterns Applied

- **Generated oracle over hand-written fixtures** (`TST-04`): the matrix asserts
  bytes, not counts, and the folded cases assert the guard rather than silence.
- **Fail closed on the unported branch** (`ENG-10`): the refusal names the shape
  and the threshold, so the next author knows exactly what to port.

## Follow-ups

- Port `foldFlowLines`' `block` mode + `blockString`'s folded pre-pass
  (newline doubling, more-indented break removal, overflow fallback), then delete
  the last `render_blockers` branch and turn the four refused cases into
  byte assertions.
