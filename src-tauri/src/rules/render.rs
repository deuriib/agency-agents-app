//! The four native rule renderers (`agency-rules/scripts/lib/renderers/*.mjs`)
//! and the YAML emitter they share.
//!
//! Each renderer is a PURE function of `(&Rule)` — no clock, no filesystem, no
//! map iteration into output — so `rendered_hash` is stable (spec §6, the same
//! contract as the agent renderers in `src-tauri/src/render/mod.rs`). Byte
//! equality with the reference is the requirement, not "equivalent YAML": the
//! ledger identifies an installed file by re-rendering it and matching bytes.
//!
//! ## Why a hand-written emitter and not `serde_yaml` (ruling A13)
//!
//! The reference wraps its frontmatter map with `yaml@2`'s `stringify` and
//! `.trimEnd()` (`_util.mjs`). Two measured behaviours make a general Rust
//! serializer the wrong tool:
//!
//! 1. Block sequences are indented two spaces (`globs:\n  - "**/*.ts"`);
//!    `serde_yaml` emits them flush (`globs:\n- "**/*.ts"`), which would fail
//!    conformance on every globbed rule.
//! 2. Plain scalars fold at 80 columns with a two-space continuation indent,
//!    and quoting follows `yaml@2`'s own indicator list (`globs: "**/*.ts"`,
//!    but `globs: src/**` unquoted).
//!
//! So this module emits only the closed field set the four harnesses declare
//! (`YamlValue` has exactly three kinds) and reproduces the reference's
//! `foldFlowLines` / `plainString` / `quotedString` decisions. Nothing here
//! accepts arbitrary YAML: an unknown field never reaches the emitter.
//!
//! ## The document template
//!
//! `wrap_rule` is `---\n<yaml>\n---\n\n<trimmed body>\n` — note the BLANK LINE
//! after the closing fence. The agent renderers emit `---\n…\n---\n<body>\n`
//! with no blank line; reusing that template fails every conformance
//! comparison.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use crate::error::AppError;
use crate::render::RenderedFile;
use crate::rules::registry;
use crate::types::{Rule, RuleGlobs, RuleHarnessMeta};

/// `yaml@2`'s default `lineWidth`.
const LINE_WIDTH: usize = 80;
/// `yaml@2`'s default `minContentWidth`, used for the fold step and for the
/// "key prefix so long that the value starts on its own line" branch.
const MIN_CONTENT_WIDTH: usize = 20;
/// `yaml@2`'s default `indent` (2 spaces) — the continuation indent and the
/// block-sequence indent.
const INDENT: &str = "  ";
/// A sequence item's own continuation indent: the item starts at `  - `, so its
/// content column is 4.
const SEQ_ITEM_INDENT: &str = "    ";

/// The only three value kinds any renderer emits.
///
/// Deliberately tiny: the field set is fixed and closed, so an enum over it is
/// smaller and more honest than a generic YAML tree (ENG-13 — no abstraction
/// without a pain point).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum YamlValue {
    /// A string: quoting and folding are decided by the emitter.
    Scalar(String),
    /// `true` / `false`, never quoted.
    Bool(bool),
    /// A block sequence of scalars; an empty vector is emitted as `[]`.
    Seq(Vec<String>),
}

/// One rule file's bytes: `---\n<yaml>\n---\n\n<trimmed body>\n`.
///
/// `fields` order is the emission order — the emitter never sorts.
pub fn wrap_rule(fields: &[(&str, YamlValue)], body: &str) -> String {
    format!("---\n{}\n---\n\n{}\n", emit_block(fields), body.trim())
}

/// Emit an ordered field list as a YAML block: no `---` fences, no trailing
/// whitespace (the reference `.trimEnd()`s the serializer output — load-bearing
/// for a `|+` block, whose kept trailing newlines the reference then drops).
pub fn emit_block(fields: &[(&str, YamlValue)]) -> String {
    let mut out = String::new();
    for (index, (key, value)) in fields.iter().enumerate() {
        if index > 0 {
            out.push('\n');
        }
        out.push_str(key);
        out.push(':');
        match value {
            YamlValue::Bool(flag) => {
                out.push(' ');
                out.push_str(if *flag { "true" } else { "false" });
            }
            YamlValue::Scalar(text) => {
                out.push(' ');
                // The first line already carries `key: `, so the fold budget
                // for it is reduced by exactly that prefix (measured:
                // `yaml@2` passes `indentAtStart = key.length + 2`).
                out.push_str(&emit_scalar(text, INDENT, Some(width(key) + 2)));
            }
            // `yaml@2` emits an empty block sequence in flow style.
            YamlValue::Seq(items) if items.is_empty() => out.push_str(" []"),
            YamlValue::Seq(items) => {
                for item in items {
                    out.push('\n');
                    out.push_str(INDENT);
                    out.push_str("- ");
                    out.push_str(&emit_scalar(item, SEQ_ITEM_INDENT, None));
                }
            }
        }
    }
    out.trim_end().to_string()
}

// =========================================================
// Renderers — one pure function per harness
// =========================================================

/// Antigravity (`md-yaml-trigger`): `trigger` is mandatory and derived by
/// precedence; `globs` is emitted only under `trigger: glob`, joined with
/// commas into a single scalar; `description` when non-empty.
///
/// `name`, `alwaysApply`, `match`, `tags` and `tools` are never emitted.
pub(crate) fn render_antigravity(rule: &Rule) -> String {
    let trigger = antigravity_trigger(rule);
    let mut fields: Vec<(&str, YamlValue)> = Vec::with_capacity(3);
    fields.push(("trigger", YamlValue::Scalar(trigger.to_string())));
    if trigger == "glob" {
        if let Some(globs) = rule.globs.as_ref() {
            fields.push(("globs", YamlValue::Scalar(globs_joined(globs))));
        }
    }
    if !rule.description.is_empty() {
        fields.push(("description", YamlValue::Scalar(rule.description.clone())));
    }
    wrap_rule(&fields, &rule.body)
}

/// Antigravity's trigger precedence, in the reference's order:
///
/// ```text
/// explicit `trigger` present  → verbatim
/// else alwaysApply == true    → "always_on"
/// else globs present          → "glob"
/// else description non-empty  → "model_decision"
/// else                        → "manual"
/// ```
///
/// The reference guards with `if (!trigger)`, so an empty `trigger: ""` counts
/// as absent; likewise `if (d.description)` is string truthiness, which for a
/// string means "not empty" (the schema forbids an empty description anyway).
/// `globs` is tested for truthiness too — an authored empty string would be
/// falsy — but [`RuleGlobs`] cannot hold one: `rules-schema.json` requires
/// `minLength: 1` / `minItems: 1`, so presence and truthiness coincide here.
fn antigravity_trigger(rule: &Rule) -> &str {
    if let Some(trigger) = rule.trigger.as_deref() {
        if !trigger.is_empty() {
            return trigger;
        }
    }
    if rule.always_apply == Some(true) {
        return "always_on";
    }
    if rule.globs.is_some() {
        return "glob";
    }
    if !rule.description.is_empty() {
        return "model_decision";
    }
    "manual"
}

/// pi-agent (`md-rules-dir`): `name`, `description`, and `tools` only when it is
/// a non-empty array. `globs`, `alwaysApply`, `trigger`, `match` and `tags` are
/// dropped, and the directory nesting (`{slug}/RULES.md`) comes from the
/// destination template — not from here.
pub(crate) fn render_pi_agent(rule: &Rule) -> String {
    let mut fields: Vec<(&str, YamlValue)> = Vec::with_capacity(3);
    fields.push(("name", YamlValue::Scalar(rule.name.clone())));
    fields.push(("description", YamlValue::Scalar(rule.description.clone())));
    if !rule.tools.is_empty() {
        fields.push(("tools", YamlValue::Seq(rule.tools.clone())));
    }
    wrap_rule(&fields, &rule.body)
}

/// oh-my-pi (`md-or-mdc`): `name`, `description`, then `globs` and
/// `alwaysApply` keyed on PRESENCE, not truthiness — `alwaysApply: false` is
/// emitted, because `false` and absent mean different things to the harness.
/// `trigger`, `match`, `tags` and `tools` are dropped.
pub(crate) fn render_oh_my_pi(rule: &Rule) -> String {
    let mut fields: Vec<(&str, YamlValue)> = Vec::with_capacity(4);
    fields.push(("name", YamlValue::Scalar(rule.name.clone())));
    fields.push(("description", YamlValue::Scalar(rule.description.clone())));
    if let Some(globs) = rule.globs.as_ref() {
        fields.push(("globs", globs_value(globs)));
    }
    if let Some(always_apply) = rule.always_apply {
        fields.push(("alwaysApply", YamlValue::Bool(always_apply)));
    }
    wrap_rule(&fields, &rule.body)
}

/// OpenCode (`md-plugin`): `description`, then `globs` and `match` when
/// present. `name`, `alwaysApply`, `trigger`, `tags` and `tools` are dropped.
///
/// Pure by ruling C1: the `opencode-rules` plugin registration in
/// `opencode.json` belongs to `rules/opencode.rs`, never to a renderer.
pub(crate) fn render_opencode(rule: &Rule) -> String {
    let mut fields: Vec<(&str, YamlValue)> = Vec::with_capacity(3);
    fields.push(("description", YamlValue::Scalar(rule.description.clone())));
    if let Some(globs) = rule.globs.as_ref() {
        fields.push(("globs", globs_value(globs)));
    }
    if let Some(matched) = rule.match_.as_ref() {
        fields.push(("match", YamlValue::Scalar(matched.clone())));
    }
    wrap_rule(&fields, &rule.body)
}

/// Both authored `globs` shapes are re-emitted as authored: a string stays a
/// scalar, an array stays a block sequence.
fn globs_value(globs: &RuleGlobs) -> YamlValue {
    match globs {
        RuleGlobs::One(one) => YamlValue::Scalar(one.clone()),
        RuleGlobs::Many(many) => YamlValue::Seq(many.clone()),
    }
}

/// Antigravity wants ONE comma-joined string, with no space after the comma,
/// whatever shape was authored.
fn globs_joined(globs: &RuleGlobs) -> String {
    match globs {
        RuleGlobs::One(one) => one.clone(),
        RuleGlobs::Many(many) => many.join(","),
    }
}

/// Fields this emitter cannot yet compile byte-for-byte.
///
/// Multi-line values are supported (literal block, chomping, indent indicator,
/// quoted fallbacks — proven byte-identical against the reference compiler in
/// `block_scalars_match_the_reference_compiler`). One shape still is not: a
/// value with a line longer than `lineWidth - indent`, which `yaml@2` folds into
/// a `>` block. Its folding rules (blank-line doubling, more-indented blocks,
/// overflow fallback) are not ported, so writing those bytes would make an
/// install disagree with upstream for a reason nobody authored — exactly what
/// `rendered_hash` exists to catch. Callers MUST refuse a rule this reports.
///
/// Empty vector ⇒ renderable. A carriage return is not a blocker: the reference
/// quotes such a value, and so does [`block_scalar`].
pub fn render_blockers(rule: &Rule) -> Vec<String> {
    let mut blockers = Vec::new();
    push_unrenderable(&mut blockers, "description", &rule.description);
    push_unrenderable(&mut blockers, "name", &rule.name);
    if let Some(trigger) = rule.trigger.as_deref() {
        push_unrenderable(&mut blockers, "trigger", trigger);
    }
    if let Some(matched) = rule.match_.as_ref() {
        push_unrenderable(&mut blockers, "match", matched);
    }
    if let Some(globs) = rule.globs.as_ref() {
        match globs {
            RuleGlobs::One(one) => push_unrenderable(&mut blockers, "globs", one),
            RuleGlobs::Many(many) => {
                for glob in many {
                    push_unrenderable(&mut blockers, "globs", glob);
                }
            }
        }
    }
    for tool in &rule.tools {
        push_unrenderable(&mut blockers, "tools", tool);
    }
    blockers
}

/// Flag a value the folded branch would have to compile.
fn push_unrenderable(blockers: &mut Vec<String>, field: &str, value: &str) {
    if !value.contains('\n') || value.contains('\r') {
        return;
    }
    if line_length_over_limit(value, LINE_WIDTH, width(INDENT)) {
        blockers.push(format!(
            "`{field}` has a line longer than {} columns; the reference compiler \
             folds such a value into a `>` block, which this app cannot yet \
             compile byte-for-byte, so the rule is not installable",
            LINE_WIDTH - width(INDENT)
        ));
    }
}

// =========================================================
// Scalar emission — a faithful port of `yaml@2`'s decisions
// =========================================================

/// Render one scalar (value or sequence item), including quoting and folding.
///
/// `indent` is the continuation indent, `indent_at_start` the width already
/// consumed on the first line (the `key: ` prefix) and `None` for a sequence
/// item, which starts at a fixed column.
fn emit_scalar(value: &str, indent: &str, indent_at_start: Option<usize>) -> String {
    // A multi-line value cannot be plain or quoted without changing its bytes,
    // so the reference falls through to its block-scalar path (`blockString`):
    // literal or folded, with a chomping indicator and, when the value starts
    // with a space, an explicit indent indicator. One shape still bails out to a
    // double-quoted string, exactly as the reference does.
    if value.contains('\n') || value.contains('\r') {
        return block_scalar(value, indent);
    }
    if needs_quoting(value) {
        // `yaml@2` picks single quotes only when the value contains `"` and no
        // `'`; otherwise it double-quotes (its `singleQuote` option is unset).
        if value.contains('"') && !value.contains('\'') {
            return fold(
                &single_quoted(value),
                indent,
                indent_at_start,
                FoldMode::Flow,
            );
        }
        return fold(
            &double_quoted(value),
            indent,
            indent_at_start,
            FoldMode::Quoted,
        );
    }
    fold(value, indent, indent_at_start, FoldMode::Flow)
}

/// `yaml@2`'s `plainString` rejection test, branch for branch. A match means the
/// scalar must be quoted (or, if multi-line, written as a block).
fn needs_quoting(value: &str) -> bool {
    let chars: Vec<char> = value.chars().collect();
    // `^[\n\t ,[\]{}#&*!|>'"%@`]` — a leading indicator character.
    if let Some(&first) = chars.first() {
        if matches!(
            first,
            '\n' | '\t'
                | ' '
                | ','
                | '['
                | ']'
                | '{'
                | '}'
                | '#'
                | '&'
                | '*'
                | '!'
                | '|'
                | '>'
                | '\''
                | '"'
                | '%'
                | '@'
                | '`'
        ) {
            return true;
        }
        // `^[?-]$` and `^[?-][ \t]`.
        if matches!(first, '-' | '?') {
            match chars.get(1) {
                None => return true,
                Some(' ' | '\t') => return true,
                Some(_) => {}
            }
        }
    }
    for pair in chars.windows(2) {
        // `[\n:][ \t]` — a colon (or newline) followed by space/tab.
        if matches!(pair[0], '\n' | ':') && matches!(pair[1], ' ' | '\t') {
            return true;
        }
        // `[ \t]\n`.
        if matches!(pair[0], ' ' | '\t') && pair[1] == '\n' {
            return true;
        }
        // `[\n\t ]#`.
        if matches!(pair[0], '\n' | '\t' | ' ') && pair[1] == '#' {
            return true;
        }
    }
    // `[\n\t :]$`.
    if matches!(chars.last(), Some('\n' | '\t' | ' ' | ':')) {
        return true;
    }
    // A plain scalar that the core schema would re-read as a null, bool, int or
    // float must be quoted or the round-trip changes its type. `yes`/`no`/
    // `on`/`off` are deliberately NOT here: YAML 1.2 core has no such booleans.
    parses_as_core_scalar(value)
}

/// The core schema's `null`, `bool`, `int` (decimal/hex/octal) and `float`
/// (plain, exponent, `.inf`, `.nan`) tests — the exact set `yaml@2` consults
/// before emitting a string plainly.
fn parses_as_core_scalar(value: &str) -> bool {
    // `^(?:~|[Nn]ull|NULL)?$`
    if matches!(value, "" | "~" | "null" | "Null" | "NULL") {
        return true;
    }
    // `^(?:[Tt]rue|TRUE|[Ff]alse|FALSE)$`
    if matches!(
        value,
        "true" | "True" | "TRUE" | "false" | "False" | "FALSE"
    ) {
        return true;
    }
    looks_like_number(value)
}

fn looks_like_number(value: &str) -> bool {
    // Radix forms take no sign (`^0x…$`, `^0o…$`).
    if let Some(hex) = value.strip_prefix("0x") {
        return !hex.is_empty() && hex.chars().all(|c| c.is_ascii_hexdigit());
    }
    if let Some(octal) = value.strip_prefix("0o") {
        return !octal.is_empty() && octal.chars().all(|c| ('0'..='7').contains(&c));
    }
    let body = value.strip_prefix(['-', '+']).unwrap_or(value);
    if body.is_empty() {
        return false;
    }
    if body.chars().all(|c| c.is_ascii_digit()) {
        return true;
    }
    // `^(?:[-+]?\.(?:inf|Inf|INF)|\.nan|\.NaN|\.NAN)$` — note `.nan` takes no
    // sign, `.inf` does.
    if matches!(body, ".inf" | ".Inf" | ".INF") || matches!(value, ".nan" | ".NaN" | ".NAN") {
        return true;
    }
    // Exponent form, whose mantissa is a decimal or an integer.
    if let Some(position) = body.find(['e', 'E']) {
        let (mantissa, exponent) = (&body[..position], &body[position + 1..]);
        let exponent_body = exponent.strip_prefix(['-', '+']).unwrap_or(exponent);
        let exponent_ok =
            !exponent_body.is_empty() && exponent_body.chars().all(|c| c.is_ascii_digit());
        let mantissa_ok = is_decimal_float(mantissa)
            || (!mantissa.is_empty() && mantissa.chars().all(|c| c.is_ascii_digit()));
        return exponent_ok && mantissa_ok;
    }
    is_decimal_float(body)
}

/// The two decimal shapes the core float tag allows: `\.[0-9]+` or
/// `[0-9]+\.[0-9]*`.
fn is_decimal_float(value: &str) -> bool {
    let Some((integer, fraction)) = value.split_once('.') else {
        return false;
    };
    if fraction.contains('.') {
        return false;
    }
    let integer_ok = integer.chars().all(|c| c.is_ascii_digit());
    let fraction_ok = fraction.chars().all(|c| c.is_ascii_digit());
    if integer.is_empty() {
        // `\.[0-9]+`
        !fraction.is_empty() && fraction_ok
    } else {
        // `[0-9]+\.[0-9]*`
        integer_ok && fraction_ok
    }
}

/// Double-quoted form, with the escapes `yaml@2` keeps.
fn double_quoted(value: &str) -> String {
    let mut out = String::with_capacity(value.len() + 2);
    out.push('"');
    for ch in value.chars() {
        match ch {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\t' => out.push_str("\\t"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            control if (control as u32) < 0x20 || control as u32 == 0x7f => {
                out.push_str(&format!("\\x{:02x}", control as u32));
            }
            printable => out.push(printable),
        }
    }
    out.push('"');
    out
}

/// Single-quoted form: only `'` needs escaping, by doubling it.
fn single_quoted(value: &str) -> String {
    format!("'{}'", value.replace('\'', "''"))
}

/// `yaml@2`'s `blockString`, literal branch: compute the chomping indicator
/// from the value's trailing whitespace and add an explicit indent indicator
/// when the value starts with a space.
///
/// Two shapes still go elsewhere, both matching the reference:
///
/// - a value whose trailing whitespace sits after a newline cannot be a block at
///   all, so it becomes a double-quoted string;
/// - a value carrying a carriage return is quoted too (a block would preserve
///   the `\r` the reference escapes).
///
/// The **folded** branch (`>`) is the one shape this module still refuses: the
/// reference switches to it when a line exceeds `lineWidth - indent`, and its
/// folding rules (more-indented blocks, blank-line doubling, overflow fallback)
/// are not ported. [`render_blockers`] refuses exactly that shape, so this
/// function never has to write bytes it cannot defend.
fn block_scalar(value: &str, indent: &str) -> String {
    // `/\n[\t ]+$/` — a block would lose the trailing spaces.
    if ends_with_newline_then_whitespace(value) || value.contains('\r') {
        return fold(&double_quoted(value), indent, None, FoldMode::Quoted);
    }

    // Chomping: strip when nothing trails, keep when the trailing run is the
    // whole value or continues past its first newline, clip otherwise.
    let chars: Vec<char> = value.chars().collect();
    let mut end_start = chars.len();
    while end_start > 0 {
        let ch = chars[end_start - 1];
        if ch != '\n' && ch != '\t' && ch != ' ' {
            break;
        }
        end_start -= 1;
    }
    let mut end: String = chars[end_start..].iter().collect();
    let chomp = match end.find('\n') {
        None => "-",
        Some(pos) if end_start == 0 || pos != end.chars().count() - 1 => "+",
        Some(_) => "",
    };

    let mut body: String = chars[..end_start].iter().collect();
    if !end.is_empty() {
        if end.ends_with('\n') {
            end.pop();
        }
        end = indent_isolated_newline_runs(&end, indent);
    }

    // Leading whitespace: an indent indicator, plus the run emitted verbatim.
    let body_chars: Vec<char> = body.chars().collect();
    let mut start_with_space = false;
    let mut start_end = 0usize;
    let mut start_nl_pos: Option<usize> = None;
    while start_end < body_chars.len() {
        match body_chars[start_end] {
            ' ' => start_with_space = true,
            '\n' => start_nl_pos = Some(start_end),
            _ => break,
        }
        start_end += 1;
    }
    let start_cut = match start_nl_pos {
        Some(pos) if pos + 1 < start_end => pos + 1,
        _ => start_end,
    };
    let mut start: String = body_chars[..start_cut].iter().collect();
    if !start.is_empty() {
        body = body_chars[start_cut..].iter().collect();
        start = indent_newline_runs(&start, indent);
    }

    // Root indentation is non-empty here, so the indicator is the indent width.
    let indent_size = if indent.is_empty() { "1" } else { "2" };
    let header = format!(
        "{}{}",
        if start_with_space { indent_size } else { "" },
        chomp
    );
    let literal_body = indent_newline_runs(&body, indent);
    format!("|{header}\n{indent}{start}{literal_body}{end}")
}

/// `/\n[\t ]+$/` — the value ends with a newline followed by blank space.
fn ends_with_newline_then_whitespace(value: &str) -> bool {
    let chars: Vec<char> = value.chars().collect();
    let mut index = chars.len();
    let mut saw_blank = false;
    while index > 0 {
        match chars[index - 1] {
            ' ' | '\t' => {
                saw_blank = true;
                index -= 1;
            }
            _ => break,
        }
    }
    saw_blank && index > 0 && chars[index - 1] == '\n'
}

/// `yaml@2`'s `lineLengthOverLimit`: true when any line exceeds
/// `line_width - indent_length` columns.
fn line_length_over_limit(value: &str, line_width: usize, indent_length: usize) -> bool {
    if line_width == 0 {
        return false;
    }
    let limit = line_width.saturating_sub(indent_length);
    let chars: Vec<char> = value.chars().collect();
    if chars.len() <= limit {
        return false;
    }
    let mut start = 0usize;
    for (index, ch) in chars.iter().enumerate() {
        if *ch == '\n' {
            if index - start > limit {
                return true;
            }
            start = index + 1;
            if chars.len() - start <= limit {
                return false;
            }
        }
    }
    true
}

/// `text.replace(/\n+/g, "$&" + indent)` — one indent after every newline run.
fn indent_newline_runs(text: &str, indent: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut in_run = false;
    for ch in text.chars() {
        if ch == '\n' {
            in_run = true;
            out.push(ch);
        } else if in_run && ch != '\n' {
            in_run = false;
            out.push_str(indent);
            out.push(ch);
        } else {
            out.push(ch);
        }
    }
    if in_run {
        out.push_str(indent);
    }
    out
}

/// `text.replace(blockEndNewlines, "$&" + indent)` — the regex matches a run of
/// newlines that neither follows nor precedes another newline, i.e. the isolated
/// breaks inside a trailing-whitespace run.
fn indent_isolated_newline_runs(text: &str, indent: &str) -> String {
    let chars: Vec<char> = text.chars().collect();
    let mut out = String::with_capacity(text.len());
    let mut index = 0usize;
    while index < chars.len() {
        if chars[index] != '\n' {
            out.push(chars[index]);
            index += 1;
            continue;
        }
        let mut run_end = index;
        while run_end < chars.len() && chars[run_end] == '\n' {
            run_end += 1;
        }
        let preceded = index == 0 || chars[index - 1] != '\n';
        let followed = run_end == chars.len() || chars[run_end] == '\n';
        for ch in &chars[index..run_end] {
            out.push(*ch);
        }
        if preceded && !followed {
            out.push_str(indent);
        }
        index = run_end;
    }
    out
}

/// Which escape-aware rules govern where a fold may land.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum FoldMode {
    /// Plain and single-quoted scalars.
    Flow,
    /// Double-quoted scalars: a fold must never split an escape sequence.
    Quoted,
}

/// Line folding, ported from `yaml@2`'s `foldFlowLines` (mode `flow`/`quoted`).
///
/// Greedy, at single spaces, at `lineWidth` columns WITH the prefix already
/// consumed on the first line and the continuation indent on the rest. Lines
/// may land exactly on the width; a word that would exceed it moves down. A
/// word longer than the width is never broken (plain mode overflows).
fn fold(text: &str, indent: &str, indent_at_start: Option<usize>, mode: FoldMode) -> String {
    let chars: Vec<char> = text.chars().collect();
    // `widths[i]` = the UTF-16 width of `chars[..i]`, i.e. the JS string offset
    // of `chars[i]` — `yaml@2` measures in UTF-16 code units, so an emoji counts
    // as two columns and a fold position must be compared in the same unit.
    let widths: Vec<usize> = std::iter::once(0)
        .chain(chars.iter().scan(0usize, |total, ch| {
            *total += ch.len_utf16();
            Some(*total)
        }))
        .collect();
    let indent_width = width(indent);
    let end_step = (1 + MIN_CONTENT_WIDTH).max(1 + LINE_WIDTH.saturating_sub(indent_width));
    let total = widths.get(chars.len()).copied().unwrap_or(0);
    // A value short enough for one continuation line is never folded, however
    // long its first line looks with the key prefix in front of it.
    if total <= end_step {
        return text.to_string();
    }

    let mut end = LINE_WIDTH.saturating_sub(indent_width);
    let mut folds: Vec<usize> = Vec::new();
    match indent_at_start {
        Some(start) if start > LINE_WIDTH - 2.max(MIN_CONTENT_WIDTH) => folds.push(0),
        Some(start) => end = LINE_WIDTH - start,
        None => {}
    }

    let mut escaped_folds: Vec<usize> = Vec::new();
    let mut split: Option<usize> = None;
    let mut previous: Option<char> = None;
    let mut escape_start: isize = -1;
    let mut escape_end: isize = -1;
    let mut index: isize = -1;

    while index < chars.len() as isize - 1 {
        index += 1;
        let mut ch = chars[index as usize];

        // In quoted mode an escape sequence is skipped whole, so a fold can
        // never land between the backslash and what it escapes.
        if mode == FoldMode::Quoted && ch == '\\' {
            escape_start = index;
            let skip = match chars.get(index as usize + 1) {
                Some('x') => 3,
                Some('u') => 5,
                Some('U') => 9,
                Some(_) => 1,
                None => 0,
            };
            index = (index + skip).min(chars.len() as isize - 1);
            escape_end = index;
        }

        if ch == '\n' {
            end = widths
                .get(index as usize)
                .copied()
                .unwrap_or(0)
                .saturating_add(indent_width + end_step);
            split = None;
        } else {
            let is_breakable_space =
                ch == ' ' && matches!(previous, Some(p) if p != ' ' && p != '\n' && p != '\t');
            if is_breakable_space {
                if let Some(&next) = chars.get(index as usize + 1) {
                    if next != ' ' && next != '\n' && next != '\t' {
                        split = Some(index as usize);
                    }
                }
            }
            let position = widths.get(index as usize).copied().unwrap_or(usize::MAX);
            if position >= end {
                match split.take() {
                    Some(at) => {
                        folds.push(at);
                        end = widths
                            .get(at)
                            .copied()
                            .unwrap_or(0)
                            .saturating_add(end_step);
                    }
                    None if mode == FoldMode::Quoted => {
                        // No breakable space: consume any run of whitespace at
                        // the current position, then fold with an explicit
                        // continuation (`\` at the end of the line) so the
                        // value's bytes are preserved.
                        while matches!(previous, Some(' ' | '\t')) {
                            previous = Some(ch);
                            index += 1;
                            if index as usize >= chars.len() {
                                break;
                            }
                            ch = chars[index as usize];
                        }
                        let at: isize = if index > escape_end + 1 {
                            index - 2
                        } else {
                            escape_start - 1
                        };
                        if at < 0 {
                            // Unreachable for real field values; leave the
                            // scalar unfolded rather than invent an offset.
                            return text.to_string();
                        }
                        let at = at as usize;
                        if escaped_folds.contains(&at) {
                            return text.to_string();
                        }
                        if folds.contains(&at) {
                            return text.to_string();
                        }
                        folds.push(at);
                        escaped_folds.push(at);
                        end = widths
                            .get(at)
                            .copied()
                            .unwrap_or(0)
                            .saturating_add(end_step);
                    }
                    None => {}
                }
            }
        }
        previous = Some(ch);
    }

    let Some(&first_fold) = folds.first() else {
        return text.to_string();
    };

    let mut out = String::with_capacity(text.len() + folds.len() * (indent_width + 1));
    out.extend(&chars[..first_fold]);
    for (position, &fold_at) in folds.iter().enumerate() {
        let slice_end = folds.get(position + 1).copied().unwrap_or(chars.len());
        if fold_at == 0 {
            // The key prefix alone filled the first line: start the value on a
            // continuation line.
            out.clear();
            out.push('\n');
            out.push_str(indent);
            out.extend(&chars[..slice_end]);
        } else {
            if escaped_folds.contains(&fold_at) {
                out.extend(chars.get(fold_at));
                out.push('\\');
            }
            out.push('\n');
            out.push_str(indent);
            out.extend(chars.get(fold_at + 1..slice_end).unwrap_or(&[]));
        }
    }
    out
}

/// Width in UTF-16 code units, matching how `yaml@2` measures columns.
fn width(text: &str) -> usize {
    text.encode_utf16().count()
}

// =========================================================
// Entry point, destinations, and the harness base resolver
// =========================================================

/// The rule entry point: render ONE file for `harness`, dispatching on the
/// `format` the contract declares (mirrors `render::render_files`).
///
/// Exactly one file for every format this app implements, and `relative` stays
/// empty: the destination template (`{slug}.md`, `{slug}/RULES.md`) already
/// carries any nesting, so emitting it here would duplicate it on disk.
///
/// Fails closed (ruling C11): a rule carrying a shape the emitter cannot
/// reproduce byte-for-byte is refused before any renderer runs, with
/// [`render_blockers`]' message, rather than producing bytes that disagree with
/// upstream.
pub fn render_rule_files(rule: &Rule, harness: &str) -> Result<Vec<RenderedFile>, AppError> {
    let meta = registry::get(harness).ok_or_else(|| AppError::InvalidArgument {
        message: format!("unknown rule harness: {harness}"),
    })?;

    let blockers = render_blockers(rule);
    if !blockers.is_empty() {
        return Err(AppError::InvalidArgument {
            message: blockers.join("\n"),
        });
    }

    let bytes = render_for_format(&meta.format, rule).ok_or_else(|| AppError::InvalidArgument {
        message: format!(
            "rule harness '{harness}' has no renderer in this app (format {})",
            meta.format
        ),
    })?;

    Ok(vec![RenderedFile {
        relative: String::new(),
        bytes,
    }])
}

/// The format → renderer dispatch. `None` means the contract declares a format
/// this app has no renderer for — reachable only by adding a harness to
/// `data/rules-tools.json` without porting its renderer, which is exactly the
/// state `installable()` already hides from the UI.
fn render_for_format(format: &str, rule: &Rule) -> Option<String> {
    match format {
        "md-yaml-trigger" => Some(render_antigravity(rule)),
        "md-rules-dir" => Some(render_pi_agent(rule)),
        "md-or-mdc" => Some(render_oh_my_pi(rule)),
        "md-plugin" => Some(render_opencode(rule)),
        _ => None,
    }
}

/// `^[a-z][a-z0-9-]*$` — the schema's `name` pattern, re-checked in Rust before
/// any path join (spec §8, DENY-03).
///
/// One implementation, called by [`rule_dests`] before the join and again by
/// `rules_install` at the write boundary. A slug is the only user-controlled
/// component of a destination path, so this is the traversal gate.
pub fn validate_slug(slug: &str) -> Result<(), AppError> {
    let mut chars = slug.chars();
    let well_formed = match chars.next() {
        Some(first) if first.is_ascii_lowercase() => {
            chars.all(|ch| ch.is_ascii_lowercase() || ch.is_ascii_digit() || ch == '-')
        }
        _ => false,
    };
    // The schema caps `name` at 64 characters. Mirroring that here keeps a
    // caller that skipped schema validation from reaching the filesystem with a
    // name no filesystem can hold (`ENAMETOOLONG` is a write error, not a
    // guard).
    if well_formed && slug.len() <= 64 {
        Ok(())
    } else {
        Err(AppError::InvalidArgument {
            message: format!("invalid rule slug: {slug:?}"),
        })
    }
}

/// Absolute destination(s) for `(harness, slug, scope)`.
///
/// `home` is the USER-SCOPE base resolved by [`harness_base`] (ruling A7) —
/// not necessarily the OS home. `project_root` selects project scope and
/// ignores `home`.
///
/// Never special-cases a harness id: the template comes from the contract, and
/// `{slug}` is substituted with the validated slug. A template still holding a
/// brace after substitution (e.g. the agent registry's `{slug}`) is an error,
/// not a literal directory name.
pub fn rule_dests(
    harness: &str,
    slug: &str,
    home: &Path,
    project_root: Option<&Path>,
) -> Result<Vec<PathBuf>, AppError> {
    validate_slug(slug)?;
    let meta = registry::get(harness).ok_or_else(|| AppError::InvalidArgument {
        message: format!("unknown rule harness: {harness}"),
    })?;
    rule_dests_with(meta, slug, home, project_root)
}

/// [`rule_dests`] against a harness record the caller already resolved.
///
/// Split out so the two scope errors are unit-testable: every shipped harness
/// supports both scopes, so the shipped contract cannot reach either branch
/// (the same reason `registry::is_installable_with` exists).
fn rule_dests_with(
    meta: &RuleHarnessMeta,
    slug: &str,
    home: &Path,
    project_root: Option<&Path>,
) -> Result<Vec<PathBuf>, AppError> {
    validate_slug(slug)?;

    let (template, root) = match project_root {
        Some(root) => {
            let template =
                meta.dest
                    .project
                    .as_deref()
                    .ok_or_else(|| AppError::InvalidArgument {
                        message: format!(
                            "rule harness '{}' is user-scoped; a project path is not supported",
                            meta.id
                        ),
                    })?;
            (template, root)
        }
        None => {
            let template = meta
                .dest
                .user
                .as_deref()
                .ok_or_else(|| AppError::InvalidArgument {
                    // The agent path's wording, reused verbatim (spec §8): a
                    // project-only harness asked for user scope needs a root.
                    message: format!(
                        "rule harness '{}' is project-scoped; a project path is required",
                        meta.id
                    ),
                })?;
            (template, home)
        }
    };

    // A destination is always a relative template. An absolute one would
    // escape the selected root entirely (DENY-03).
    if Path::new(template).is_absolute() {
        return Err(AppError::InvalidArgument {
            message: format!(
                "rule harness '{}' declares an absolute destination template: {template}",
                meta.id
            ),
        });
    }

    let stripped = template.strip_prefix("~/").unwrap_or(template);
    let rendered = stripped.replace("{slug}", slug);

    // One placeholder across agents and rules (issue #68). A template that
    // still carries the legacy double-brace form resolves to `{{piAgent}}` →
    // `{piAgent}` and would create a literally braced directory that no
    // harness reads, so refuse it instead of writing it.
    if rendered.contains('{') || rendered.contains('}') {
        return Err(AppError::InvalidArgument {
            message: format!(
                "rule harness '{}' destination template must use the {{slug}} placeholder: {template}",
                meta.id
            ),
        });
    }
    // Belt and braces behind the slug gate: no template may climb out of its
    // root, and the final path must stay under the root we chose.
    if rendered
        .split(['/', '\\'])
        .any(|segment| segment == ".." || segment == ".")
    {
        return Err(AppError::InvalidArgument {
            message: format!(
                "rule harness '{}' destination template escapes its root: {template}",
                meta.id
            ),
        });
    }

    let path = root.join(&rendered);
    if !path.starts_with(root) {
        return Err(AppError::InvalidArgument {
            message: format!(
                "resolved rule destination escapes its root: {}",
                path.display()
            ),
        });
    }
    Ok(vec![path])
}

/// The USER-SCOPE base for a rule harness (ruling A7).
///
/// `settings.tool_paths[tool_id]` wins when set, by resolving the harness to an
/// agent-registry entry through its `kebab` field; otherwise the OS home. Total:
/// no panic and no error, even for a harness with no registry match (a future
/// harness, or a typo), which falls back to `os_home`.
///
/// Project scope never consults this — it roots at the picked project.
pub fn harness_base(
    tool_paths: &HashMap<String, String>,
    harness: &str,
    os_home: &Path,
) -> PathBuf {
    match registry::tool_for_harness(harness) {
        None => os_home.to_path_buf(),
        // Reuse the ONE implementation of "custom path wins, else home" instead
        // of copying it — the agent and rule paths must not drift.
        Some(tool) => crate::install::resolve_tool_base(tool_paths, &tool.id, os_home),
    }
}

// =========================================================
// Tests
// =========================================================

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::RuleValidation;

    /// A rule shaped like `rules/stacks/typescript.md`, with every optional
    /// field absent so each test opts in to exactly what it exercises.
    fn sample() -> Rule {
        Rule {
            slug: "typescript".to_string(),
            name: "typescript".to_string(),
            description: "Mandatory rules for editing TypeScript and TSX files.".to_string(),
            category: "stacks".to_string(),
            relative_path: "rules/stacks/typescript.md".to_string(),
            globs: None,
            always_apply: None,
            trigger: None,
            match_: None,
            tags: vec!["language".to_string()],
            tools: Vec::new(),
            body: "# TypeScript Rules\n\n- No `any`.".to_string(),
            source_hash: "0".repeat(64),
            body_hash: "0".repeat(64),
            validation: RuleValidation {
                valid: true,
                errors: Vec::new(),
            },
        }
    }

    /// The YAML between the fences, so an assertion about an emitted key cannot
    /// be satisfied by the same text appearing in the Markdown body.
    fn frontmatter(rendered: &str) -> &str {
        let start = rendered
            .strip_prefix("---\n")
            .unwrap_or_else(|| panic!("no opening fence: {rendered:?}"));
        let end = start
            .find("\n---\n")
            .unwrap_or_else(|| panic!("no closing fence: {rendered:?}"));
        &start[..end]
    }

    fn yaml(fields: &[(&str, YamlValue)]) -> String {
        emit_block(fields)
    }

    fn scalar(value: &str) -> String {
        yaml(&[("description", YamlValue::Scalar(value.to_string()))])
    }

    // ── the emitter: `wrap_rule` ────────────────────────────────────────────

    #[test]
    fn wrap_rule_puts_a_blank_line_after_the_closing_fence() {
        let rendered = wrap_rule(
            &[("trigger", YamlValue::Scalar("manual".to_string()))],
            "\n\n# Title\n\nbody\n\n",
        );
        assert_eq!(rendered, "---\ntrigger: manual\n---\n\n# Title\n\nbody\n");
    }

    #[test]
    fn emit_block_has_no_trailing_newline_and_keeps_caller_order() {
        let block = yaml(&[
            ("name", YamlValue::Scalar("python".to_string())),
            ("description", YamlValue::Scalar("Rules.".to_string())),
            ("tools", YamlValue::Seq(vec!["Read".to_string()])),
        ]);
        assert_eq!(block, "name: python\ndescription: Rules.\ntools:\n  - Read");
        assert!(!block.ends_with('\n'));
    }

    // ── the emitter: scalars ───────────────────────────────────────────────

    /// Table-driven over every row measured against `yaml@2` (§5.3).
    #[test]
    fn yaml_scalar_quoting_table() {
        let cases: &[(&str, &str)] = &[
            // Plain-safe.
            ("glob", "description: glob"),
            ("always_on", "description: always_on"),
            ("model_decision", "description: model_decision"),
            ("manual", "description: manual"),
            ("any", "description: any"),
            ("all", "description: all"),
            ("plain text", "description: plain text"),
            ("a b  c", "description: a b  c"),
            ("star*", "description: star*"),
            ("100%", "description: 100%"),
            ("a#b", "description: a#b"),
            ("key:value", "description: key:value"),
            ("=eq", "description: =eq"),
            ("2026-11-03", "description: 2026-11-03"),
            ("yes", "description: yes"),
            ("no", "description: no"),
            ("on", "description: on"),
            ("off", "description: off"),
            ("?q", "description: ?q"),
            ("src/**", "description: src/**"),
            // Leading `*` forces double quotes.
            ("**/*.ts", "description: \"**/*.ts\""),
            ("*.md", "description: \"*.md\""),
            ("*star", "description: \"*star\""),
            // `": "` anywhere, or a trailing colon.
            ("has: colon", "description: \"has: colon\""),
            ("a: b", "description: \"a: b\""),
            ("ends with colon:", "description: \"ends with colon:\""),
            // Leading `#`, or ` #` anywhere.
            ("#hash", "description: \"#hash\""),
            ("trail hash #", "description: \"trail hash #\""),
            // Leading/trailing space.
            (" leading", "description: \" leading\""),
            ("trailing ", "description: \"trailing \""),
            ("  ", "description: \"  \""),
            // Leading indicator characters.
            ("- dash", "description: \"- dash\""),
            ("? q", "description: \"? q\""),
            ("%p", "description: \"%p\""),
            ("!bang", "description: \"!bang\""),
            ("&amp", "description: \"&amp\""),
            ("|pipe", "description: \"|pipe\""),
            (">gt", "description: \">gt\""),
            ("[a]", "description: \"[a]\""),
            ("{a}", "description: \"{a}\""),
            ("@at", "description: \"@at\""),
            ("`tick", "description: \"`tick\""),
            ("'single'", "description: \"'single'\""),
            // Empty, and anything the core schema would re-read as a type.
            ("", "description: \"\""),
            ("true", "description: \"true\""),
            ("false", "description: \"false\""),
            ("null", "description: \"null\""),
            ("~", "description: \"~\""),
            ("123", "description: \"123\""),
            ("-1", "description: \"-1\""),
            ("1.5", "description: \"1.5\""),
            ("1e9", "description: \"1e9\""),
            ("0x1f", "description: \"0x1f\""),
            // A value with `"` but no `'` is single-quoted.
            ("\"quoted\"", "description: '\"quoted\"'"),
            // Both quote kinds and no other trigger: plain, verbatim.
            ("it's \"x\"", "description: it's \"x\""),
        ];
        for (input, expected) in cases {
            assert_eq!(&scalar(input), expected, "input {input:?}");
        }
    }

    #[test]
    fn yaml_emits_bools_unquoted() {
        let block = yaml(&[
            ("alwaysApply", YamlValue::Bool(false)),
            ("other", YamlValue::Bool(true)),
        ]);
        assert_eq!(block, "alwaysApply: false\nother: true");
    }

    #[test]
    fn yaml_indents_block_sequences_by_two_spaces() {
        let block = yaml(&[(
            "globs",
            YamlValue::Seq(vec!["**/*.ts".to_string(), "**/*.tsx".to_string()]),
        )]);
        assert_eq!(block, "globs:\n  - \"**/*.ts\"\n  - \"**/*.tsx\"");
    }

    /// A sequence item that needs no quotes must not be quoted: "always quote
    /// globs" would pass every fixture today and diverge the first time a rule
    /// authors `globs: src/**`.
    #[test]
    fn yaml_leaves_unambiguous_sequence_items_plain() {
        let block = yaml(&[
            (
                "globs",
                YamlValue::Seq(vec!["src/**".to_string(), "**/*.ts".to_string()]),
            ),
            ("tools", YamlValue::Seq(vec!["Read".to_string()])),
        ]);
        assert_eq!(
            block,
            "globs:\n  - src/**\n  - \"**/*.ts\"\ntools:\n  - Read"
        );
    }

    #[test]
    fn yaml_emits_an_empty_block_sequence_in_flow_style() {
        assert_eq!(yaml(&[("globs", YamlValue::Seq(Vec::new()))]), "globs: []");
    }

    // ── the emitter: folding ───────────────────────────────────────────────

    /// The one shape four of the 28 conformance comparisons exercise.
    #[test]
    fn yaml_folds_at_80_columns() {
        let description = "Apply these guidelines for Nexus Strategy activation, runbooks, \
                           playbooks, and coordination.";
        assert_eq!(
            scalar(description),
            "description: Apply these guidelines for Nexus Strategy activation, runbooks,\n  \
             playbooks, and coordination."
        );
    }

    /// Greedy at the last space, not hard at the column: a first line that
    /// lands on exactly 80 columns keeps the word that fills it.
    #[test]
    fn yaml_does_not_fold_a_line_that_lands_on_80_columns() {
        let description = format!("{} {}", "a".repeat(67), "b".repeat(20));
        assert_eq!(
            scalar(&description),
            format!("description: {}\n  {}", "a".repeat(67), "b".repeat(20))
        );
    }

    /// …and 81 columns is one too many, so the word moves down.
    #[test]
    fn yaml_folds_a_line_that_would_reach_81_columns() {
        let description = format!("{} {}", "a".repeat(68), "b".repeat(20));
        assert_eq!(
            scalar(&description),
            format!("description: {}\n  {}", "a".repeat(68), "b".repeat(20))
        );
    }

    /// A value of up to 79 characters is never folded, even when the key prefix
    /// pushes the physical line past the width — `yaml@2` guards the fold with
    /// the value's own length.
    #[test]
    fn yaml_does_not_fold_a_value_below_the_width_guard() {
        let description = "a".repeat(79);
        assert_eq!(scalar(&description), format!("description: {description}"));
        assert_eq!(scalar(&description).chars().count(), 13 + 79);
    }

    #[test]
    fn yaml_does_not_break_long_words() {
        let description = "a".repeat(120);
        assert_eq!(scalar(&description), format!("description: {description}"));
    }

    #[test]
    fn yaml_folds_a_multi_word_value_across_three_lines() {
        // Word widths chosen so the greedy break lands twice.
        let description: String = (0..40)
            .map(|index| format!("w{index}"))
            .collect::<Vec<_>>()
            .join(" ");
        assert_eq!(
            scalar(&description),
            "description: w0 w1 w2 w3 w4 w5 w6 w7 w8 w9 w10 w11 w12 w13 w14 w15 w16 w17 w18\n  \
             w19 w20 w21 w22 w23 w24 w25 w26 w27 w28 w29 w30 w31 w32 w33 w34 w35 w36 w37\n  \
             w38 w39"
        );
    }

    /// A quoted scalar folds too, and the closing quote rides the last line.
    #[test]
    fn yaml_folds_a_double_quoted_scalar() {
        let words = |count: usize| vec!["word"; count].join(" ");
        let description = format!("Note: {}", words(20));
        assert_eq!(
            scalar(&description),
            format!("description: \"Note: {}\n  {}\"", words(12), words(8))
        );
    }

    /// No breakable space inside a quoted scalar: the fold must not split the
    /// value, so it ends the line with an explicit `\` continuation.
    #[test]
    fn yaml_folds_a_quoted_scalar_without_spaces_using_a_continuation() {
        let description = format!("*{}", "a".repeat(120));
        let rendered = scalar(&description);
        assert!(
            rendered.contains("\\\n  "),
            "expected a backslash continuation: {rendered:?}"
        );
        assert_eq!(
            rendered,
            format!(
                "description: \"*{}\\\n  {}\"",
                "a".repeat(64),
                "a".repeat(56)
            )
        );
    }

    /// A sequence item folds at its own column: the item's content starts at
    /// column 4 (`  - `), and continuations indent to that same column.
    #[test]
    fn yaml_folds_a_long_sequence_item_at_its_own_column() {
        let words = |count: usize| vec!["word"; count].join(" ");
        let block = yaml(&[("globs", YamlValue::Seq(vec![words(20)]))]);
        assert_eq!(
            block,
            format!("globs:\n  - {}\n    {}", words(15), words(5))
        );
    }

    /// A sequence item with nothing to break on overflows instead of being
    /// split mid-word.
    #[test]
    fn yaml_does_not_break_an_unbreakable_sequence_item() {
        let glob = format!("src/{}*.ts", "a/".repeat(40));
        let block = yaml(&[("globs", YamlValue::Seq(vec![glob.clone()]))]);
        assert_eq!(block, format!("globs:\n  - {glob}"));
    }

    /// A multi-line value cannot stay plain: the reference writes a literal
    /// block, with `-` when the value has no trailing newline.
    #[test]
    fn yaml_writes_a_multiline_value_as_a_literal_block() {
        assert_eq!(scalar("a\nb"), "description: |-\n  a\n  b");
        assert_eq!(scalar("a\nb\n"), "description: |\n  a\n  b");
        assert_eq!(scalar(" a\nb"), "description: |2-\n   a\n  b");
    }

    // ── Antigravity ────────────────────────────────────────────────────────

    #[test]
    fn antigravity_uses_an_explicit_trigger_verbatim() {
        let mut rule = sample();
        rule.trigger = Some("manual".to_string());
        rule.always_apply = Some(true);
        rule.globs = Some(RuleGlobs::Many(vec!["**/*.ts".to_string()]));
        let rendered = render_antigravity(&rule);
        assert_eq!(
            frontmatter(&rendered),
            "trigger: manual\ndescription: Mandatory rules for editing TypeScript and TSX files."
        );
    }

    #[test]
    fn antigravity_maps_always_apply_true_to_always_on() {
        let mut rule = sample();
        rule.always_apply = Some(true);
        rule.globs = Some(RuleGlobs::Many(vec!["**/*.ts".to_string()]));
        let rendered = render_antigravity(&rule);
        assert_eq!(
            frontmatter(&rendered),
            "trigger: always_on\ndescription: Mandatory rules for editing TypeScript and TSX files."
        );
        // `alwaysApply` is not a field this harness understands.
        assert!(!frontmatter(&rendered).contains("alwaysApply"));
        // And `globs` is only emitted under `trigger: glob`.
        assert!(!frontmatter(&rendered).contains("globs"));
    }

    #[test]
    fn antigravity_maps_globs_to_the_glob_trigger() {
        let mut rule = sample();
        rule.globs = Some(RuleGlobs::One("**/*.ts".to_string()));
        let rendered = render_antigravity(&rule);
        assert_eq!(
            frontmatter(&rendered),
            "trigger: glob\nglobs: \"**/*.ts\"\n\
             description: Mandatory rules for editing TypeScript and TSX files."
        );
    }

    #[test]
    fn antigravity_maps_a_description_only_rule_to_model_decision() {
        let rendered = render_antigravity(&sample());
        assert_eq!(
            frontmatter(&rendered),
            "trigger: model_decision\n\
             description: Mandatory rules for editing TypeScript and TSX files."
        );
    }

    #[test]
    fn antigravity_maps_an_empty_rule_to_manual() {
        let mut rule = sample();
        rule.description = String::new();
        let rendered = render_antigravity(&rule);
        assert_eq!(frontmatter(&rendered), "trigger: manual");
    }

    #[test]
    fn antigravity_always_apply_false_falls_through_to_globs() {
        let mut rule = sample();
        rule.always_apply = Some(false);
        rule.globs = Some(RuleGlobs::One("**/*.ts".to_string()));
        assert!(frontmatter(&render_antigravity(&rule)).starts_with("trigger: glob\n"));
    }

    #[test]
    fn antigravity_joins_array_globs_with_commas_and_no_space() {
        let mut rule = sample();
        rule.globs = Some(RuleGlobs::Many(vec![
            "**/*.ts".to_string(),
            "**/*.tsx".to_string(),
        ]));
        let rendered = render_antigravity(&rule);
        assert!(frontmatter(&rendered).contains("globs: \"**/*.ts,**/*.tsx\""));
        assert_eq!(
            frontmatter(&rendered),
            "trigger: glob\nglobs: \"**/*.ts,**/*.tsx\"\n\
             description: Mandatory rules for editing TypeScript and TSX files."
        );
    }

    #[test]
    fn antigravity_passes_a_single_string_glob_through_unchanged() {
        let mut rule = sample();
        rule.globs = Some(RuleGlobs::One("**/*.py".to_string()));
        let rendered = render_antigravity(&rule);
        assert_eq!(
            frontmatter(&rendered),
            "trigger: glob\nglobs: \"**/*.py\"\ndescription: Rules for Python modules."
                .replace("Rules for Python modules.", &rule.description)
        );
    }

    #[test]
    fn antigravity_drops_name_tags_tools_match_and_always_apply() {
        let mut rule = sample();
        rule.tools = vec!["Read".to_string()];
        rule.match_ = Some("all".to_string());
        rule.tags = vec!["language".to_string()];
        rule.always_apply = Some(true);
        let rendered = render_antigravity(&rule);
        let fields = frontmatter(&rendered);
        for dropped in ["name:", "tags:", "tools:", "match:", "alwaysApply:"] {
            assert!(
                !fields.contains(dropped),
                "antigravity must drop {dropped}: {fields:?}"
            );
        }
    }

    #[test]
    fn antigravity_is_deterministic() {
        let mut rule = sample();
        rule.globs = Some(RuleGlobs::Many(vec!["**/*.ts".to_string()]));
        assert_eq!(render_antigravity(&rule), render_antigravity(&rule));
    }

    // ── pi-agent ───────────────────────────────────────────────────────────

    #[test]
    fn pi_agent_emits_name_description_and_tools() {
        let mut rule = sample();
        rule.tools = vec!["Read".to_string(), "Write".to_string()];
        let rendered = render_pi_agent(&rule);
        assert_eq!(
            frontmatter(&rendered),
            "name: typescript\n\
             description: Mandatory rules for editing TypeScript and TSX files.\n\
             tools:\n  - Read\n  - Write"
        );
    }

    #[test]
    fn pi_agent_omits_an_empty_tools_list() {
        let mut rule = sample();
        rule.tools = Vec::new();
        let rendered = render_pi_agent(&rule);
        assert_eq!(
            frontmatter(&rendered),
            "name: typescript\n\
             description: Mandatory rules for editing TypeScript and TSX files."
        );
        assert!(!frontmatter(&rendered).contains("tools"));
    }

    #[test]
    fn pi_agent_drops_globs_always_apply_trigger_match_and_tags() {
        let mut rule = sample();
        rule.globs = Some(RuleGlobs::Many(vec!["**/*.ts".to_string()]));
        rule.always_apply = Some(true);
        rule.trigger = Some("glob".to_string());
        rule.match_ = Some("any".to_string());
        rule.tags = vec!["language".to_string()];
        let fields = frontmatter(&render_pi_agent(&rule)).to_string();
        for dropped in ["globs:", "alwaysApply:", "trigger:", "match:", "tags:"] {
            assert!(
                !fields.contains(dropped),
                "pi-agent must drop {dropped}: {fields:?}"
            );
        }
        assert_eq!(
            fields,
            "name: typescript\n\
             description: Mandatory rules for editing TypeScript and TSX files."
        );
    }

    #[test]
    fn pi_agent_is_deterministic() {
        let rule = sample();
        assert_eq!(render_pi_agent(&rule), render_pi_agent(&rule));
    }

    // ── oh-my-pi ───────────────────────────────────────────────────────────

    #[test]
    fn oh_my_pi_emits_always_apply_false_rather_than_dropping_it() {
        let mut rule = sample();
        rule.always_apply = Some(false);
        let rendered = render_oh_my_pi(&rule);
        assert!(frontmatter(&rendered).contains("alwaysApply: false"));
        assert_eq!(
            frontmatter(&rendered),
            "name: typescript\n\
             description: Mandatory rules for editing TypeScript and TSX files.\n\
             alwaysApply: false"
        );
    }

    #[test]
    fn oh_my_pi_emits_always_apply_true() {
        let mut rule = sample();
        rule.always_apply = Some(true);
        assert!(frontmatter(&render_oh_my_pi(&rule)).contains("alwaysApply: true"));
    }

    #[test]
    fn oh_my_pi_omits_always_apply_when_absent() {
        let rendered = render_oh_my_pi(&sample());
        assert!(!frontmatter(&rendered).contains("alwaysApply"));
    }

    #[test]
    fn oh_my_pi_preserves_array_globs_as_a_block_sequence() {
        let mut rule = sample();
        rule.globs = Some(RuleGlobs::Many(vec![
            "**/*.ts".to_string(),
            "**/*.tsx".to_string(),
        ]));
        assert_eq!(
            frontmatter(&render_oh_my_pi(&rule)),
            "name: typescript\n\
             description: Mandatory rules for editing TypeScript and TSX files.\n\
             globs:\n  - \"**/*.ts\"\n  - \"**/*.tsx\""
        );
    }

    /// A one-element array stays a block sequence: the source's flow style is
    /// not what gets emitted.
    #[test]
    fn oh_my_pi_preserves_a_single_element_array_glob() {
        let mut rule = sample();
        rule.name = "python".to_string();
        rule.slug = "python".to_string();
        rule.description = "Rules for Python modules.".to_string();
        rule.globs = Some(RuleGlobs::Many(vec!["**/*.py".to_string()]));
        assert_eq!(
            frontmatter(&render_oh_my_pi(&rule)),
            "name: python\ndescription: Rules for Python modules.\nglobs:\n  - \"**/*.py\""
        );
    }

    #[test]
    fn oh_my_pi_emits_a_string_glob_as_a_scalar() {
        let mut rule = sample();
        rule.globs = Some(RuleGlobs::One("src/**".to_string()));
        assert_eq!(
            frontmatter(&render_oh_my_pi(&rule)),
            "name: typescript\n\
             description: Mandatory rules for editing TypeScript and TSX files.\n\
             globs: src/**"
        );
    }

    #[test]
    fn oh_my_pi_drops_trigger_match_tags_and_tools() {
        let mut rule = sample();
        rule.trigger = Some("glob".to_string());
        rule.match_ = Some("all".to_string());
        rule.tools = vec!["Read".to_string()];
        let fields = frontmatter(&render_oh_my_pi(&rule)).to_string();
        for dropped in ["trigger:", "match:", "tags:", "tools:"] {
            assert!(
                !fields.contains(dropped),
                "oh-my-pi must drop {dropped}: {fields:?}"
            );
        }
    }

    #[test]
    fn oh_my_pi_is_deterministic() {
        let rule = sample();
        assert_eq!(render_oh_my_pi(&rule), render_oh_my_pi(&rule));
    }

    // ── OpenCode ───────────────────────────────────────────────────────────

    #[test]
    fn opencode_emits_description_globs_and_match() {
        let mut rule = sample();
        rule.globs = Some(RuleGlobs::Many(vec![
            "**/*.ts".to_string(),
            "**/*.tsx".to_string(),
        ]));
        rule.match_ = Some("all".to_string());
        assert_eq!(
            frontmatter(&render_opencode(&rule)),
            "description: Mandatory rules for editing TypeScript and TSX files.\n\
             globs:\n  - \"**/*.ts\"\n  - \"**/*.tsx\"\n\
             match: all"
        );
    }

    #[test]
    fn opencode_omits_match_when_absent() {
        let rendered = render_opencode(&sample());
        assert!(!frontmatter(&rendered).contains("match"));
        assert_eq!(
            frontmatter(&rendered),
            "description: Mandatory rules for editing TypeScript and TSX files."
        );
    }

    #[test]
    fn opencode_emits_a_string_glob_as_a_scalar() {
        let mut rule = sample();
        rule.globs = Some(RuleGlobs::One("**/*.py".to_string()));
        assert_eq!(
            frontmatter(&render_opencode(&rule)),
            "description: Mandatory rules for editing TypeScript and TSX files.\n\
             globs: \"**/*.py\""
        );
    }

    #[test]
    fn opencode_drops_name_always_apply_trigger_tags_and_tools() {
        let mut rule = sample();
        rule.always_apply = Some(true);
        rule.trigger = Some("glob".to_string());
        rule.tools = vec!["Read".to_string()];
        let fields = frontmatter(&render_opencode(&rule)).to_string();
        for dropped in ["name:", "alwaysApply:", "trigger:", "tags:", "tools:"] {
            assert!(
                !fields.contains(dropped),
                "opencode must drop {dropped}: {fields:?}"
            );
        }
    }

    #[test]
    fn opencode_is_deterministic() {
        let rule = sample();
        assert_eq!(render_opencode(&rule), render_opencode(&rule));
    }

    /// The rendered body is the parsed body, verbatim: `parse_rule` already
    /// normalized CRLF and trimmed, and the hash in the ledger covers exactly
    /// those bytes — so the renderer must not normalize again, only emit.
    #[test]
    fn the_rendered_body_is_the_normalized_body_verbatim() {
        let source = "---\nname: crlf\ndescription: d\n---\n\r\n# Title\r\n\r\nbody\r\n\r\n";
        let parsed = crate::rules::parse::parse_rule("rules/base/crlf.md", source.as_bytes())
            .expect("the CRLF rule parses");
        assert_eq!(parsed.body, "# Title\n\nbody");
        assert_eq!(
            parsed.body_hash,
            crate::render::sha256_hex(parsed.body.as_bytes())
        );

        for rendered in [
            render_antigravity(&parsed),
            render_pi_agent(&parsed),
            render_oh_my_pi(&parsed),
            render_opencode(&parsed),
        ] {
            assert!(
                rendered.ends_with(&format!("---\n\n{}\n", parsed.body)),
                "body must be wrapped verbatim: {rendered:?}"
            );
            assert!(
                !rendered.contains('\r'),
                "no CRLF may survive: {rendered:?}"
            );
        }
    }

    // ── purity guard ───────────────────────────────────────────────────────

    /// Renderers must stay pure: a filesystem read here would make
    /// `rendered_hash` depend on something other than the rule.
    #[test]
    fn renderers_have_no_filesystem_or_clock_access() {
        const SELF_SOURCE: &str = include_str!("render.rs");
        let production: String = SELF_SOURCE
            .split("#[cfg(test)]")
            .next()
            .unwrap_or(SELF_SOURCE)
            .lines()
            .filter(|line| !line.trim_start().starts_with("//"))
            .collect::<Vec<_>>()
            .join("\n");
        for forbidden in [
            "std::fs",
            "tokio::fs",
            "File::open",
            "SystemTime",
            "Instant::now",
        ] {
            assert!(
                !production.contains(forbidden),
                "rules/render.rs must stay pure; found {forbidden:?} in production code"
            );
        }
        for forbidden in ["unwrap()", "expect("] {
            assert!(
                !production.contains(forbidden),
                "rules/render.rs must not fail on rule data; found {forbidden:?}"
            );
        }
    }

    // ── render_blockers: fail closed on shapes the emitter cannot match ────

    #[test]
    fn render_blockers_accept_a_single_line_rule() {
        let mut rule = sample();
        rule.description = "Single line.".to_string();
        assert!(render_blockers(&rule).is_empty());
    }

    /// Issue #63: the literal block covers short multi-line values, so the
    /// blanket multi-line guard is gone.
    #[test]
    fn render_blockers_accept_a_short_multiline_description() {
        let mut rule = sample();
        rule.description = "first line\nsecond line".to_string();
        assert!(render_blockers(&rule).is_empty());
    }

    /// The one shape still refused, on every field the rule can carry.
    #[test]
    fn render_blockers_refuse_a_line_that_the_reference_would_fold() {
        let long = "alpha ".repeat(20).trim().to_string();
        let mut description = sample();
        description.description = format!("short\n{long}");
        assert_eq!(render_blockers(&description).len(), 1);

        // A long SINGLE-line value is not a block scalar at all: it folds as a
        // flow scalar, which this emitter already reproduces.
        let mut single_line = sample();
        single_line.globs = Some(RuleGlobs::One(long.clone()));
        assert!(render_blockers(&single_line).is_empty());

        let mut one = sample();
        one.globs = Some(RuleGlobs::One(format!("a\n{long}")));
        assert_eq!(render_blockers(&one).len(), 1);

        let mut many = sample();
        many.globs = Some(RuleGlobs::Many(vec![
            "ok".to_string(),
            format!("a\n{long}"),
        ]));
        assert_eq!(render_blockers(&many).len(), 1);
    }

    /// A carriage return is not a blocker: the reference quotes it, and so does
    /// the block path (proven in the oracle matrix).
    #[test]
    fn render_blockers_accept_a_carriage_return() {
        let mut rule = sample();
        rule.description = "a\rb".to_string();
        assert!(render_blockers(&rule).is_empty());
        assert_eq!(
            emit_block(&[("description", YamlValue::Scalar(rule.description.clone()))]),
            "description: \"a\\rb\""
        );
    }

    // ── dropped fields: the body cannot satisfy the assertion ──────────────

    #[test]
    fn a_body_repeating_dropped_keys_does_not_leak_into_the_frontmatter() {
        let mut rule = sample();
        rule.globs = Some(RuleGlobs::Many(vec!["**/*.ts".to_string()]));
        rule.always_apply = Some(true);
        rule.trigger = Some("glob".to_string());
        rule.match_ = Some("any".to_string());
        rule.body = "globs:\nname: alwaysApply: false\n---\ntrigger: glob".to_string();

        let rendered = render_pi_agent(&rule);
        let fm = frontmatter(&rendered);
        for dropped in ["globs:", "alwaysApply:", "trigger:", "match:", "tags:"] {
            assert!(!fm.contains(dropped), "{dropped} leaked into {fm:?}");
        }
        // The body keeps its own text, so the assertion above is not vacuous.
        assert!(rendered.contains("globs:"), "body must keep its own text");
    }

    // ── vendored adversarial fold cases (measured against `yaml@2`) ────────

    #[test]
    fn a_seventy_nine_character_value_is_never_folded() {
        // The reference bails on `text.length <= 79` before consulting the
        // column budget, so the emitted line may exceed 80 columns.
        let value = "x".repeat(79);
        let out = scalar(&value);
        assert_eq!(out, format!("description: {value}"));
        assert!(out.len() > 80, "the line is long on purpose: {}", out.len());
    }

    #[test]
    fn an_eighty_character_value_folds() {
        let value = format!("{} {}", "x".repeat(70), "y".repeat(9));
        assert_eq!(value.len(), 80);
        let out = scalar(&value);
        assert!(out.contains('\n'), "an 80-char value must fold: {out:?}");
    }

    #[test]
    fn a_quoted_scalar_without_spaces_folds_with_a_continuation() {
        // A leading `*` forces quoting, and there is no breakable space, so the
        // reference folds inside the word and marks it with a trailing `\`.
        let value = format!("*{}", "a".repeat(120));
        let out = scalar(&value);
        assert!(
            out.lines().any(|line| line.ends_with('\\')),
            "quoted fold needs a continuation: {out:?}"
        );
    }

    #[test]
    fn a_sequence_item_continuation_is_indented_four_spaces() {
        let value = format!("{} {}", "a".repeat(60), "b".repeat(40));
        let block = yaml(&[("tools", YamlValue::Seq(vec![value]))]);
        assert!(block.contains("\n  - "), "{block:?}");
        assert!(
            block.contains("\n    "),
            "a continuation must indent 4 spaces, not 2: {block:?}"
        );
    }

    // ── entry point: one file, dispatched on the contract's format ─────────

    #[test]
    fn every_harness_renders_exactly_one_file_with_no_relative_suffix() {
        for harness in registry::all() {
            let files = render_rule_files(&sample(), &harness.id)
                .unwrap_or_else(|e| panic!("{} must render: {e:?}", harness.id));
            assert_eq!(files.len(), 1, "{} renders one file", harness.id);
            assert!(
                files[0].relative.is_empty(),
                "{}: the dest template carries the nesting, not `relative`",
                harness.id
            );
            assert!(
                files[0].bytes.starts_with("---\n"),
                "{} rendered a document: {:?}",
                harness.id,
                files[0].bytes
            );
        }
    }

    #[test]
    fn render_rule_files_matches_the_individual_renderer() {
        let mut rule = sample();
        rule.globs = Some(RuleGlobs::Many(vec!["**/*.ts".to_string()]));
        rule.always_apply = Some(false);
        for (harness, expected) in [
            ("antigravity", render_antigravity(&rule)),
            ("piAgent", render_pi_agent(&rule)),
            ("ohMyPi", render_oh_my_pi(&rule)),
            ("opencode", render_opencode(&rule)),
        ] {
            let files = render_rule_files(&rule, harness).expect("renderable");
            assert_eq!(files[0].bytes, expected, "{harness}");
        }
    }

    #[test]
    fn render_rule_files_rejects_an_unknown_harness() {
        let error = render_rule_files(&sample(), "not-a-harness").expect_err("must refuse");
        assert!(
            matches!(&error, AppError::InvalidArgument { message } if message == "unknown rule harness: not-a-harness"),
            "{error:?}"
        );
    }

    /// A format the contract declares but this app has not ported. Unreachable
    /// with the bundled contract (every harness resolves), so exercise the
    /// dispatch directly rather than leaving the branch untested.
    #[test]
    fn an_unimplemented_format_has_no_renderer() {
        assert!(render_for_format("md-future-format", &sample()).is_none());
        for format in ["md-yaml-trigger", "md-rules-dir", "md-or-mdc", "md-plugin"] {
            assert!(
                render_for_format(format, &sample()).is_some(),
                "{format} must dispatch to a renderer"
            );
        }
    }

    /// Issue #63: a short multi-line value is now installable, and the bytes are
    /// the reference's literal block — not a refusal.
    #[test]
    fn a_short_multiline_description_renders_as_a_literal_block() {
        let mut rule = sample();
        rule.description = "first line\nsecond line".to_string();
        for harness in registry::all() {
            let files = render_rule_files(&rule, &harness.id).expect("renderable");
            let bytes = files[0].bytes.clone();
            assert!(
                bytes.contains("description: |-\n  first line\n  second line"),
                "{} did not get a literal block: {bytes}",
                harness.id
            );
        }
    }

    /// Ruling C11: the entry point refuses a rule whose frontmatter the emitter
    /// cannot reproduce byte-for-byte, with `render_blockers`' own message.
    #[test]
    fn render_rule_files_refuses_a_blocked_rule_on_every_harness() {
        let mut rule = sample();
        rule.description = format!("short\n{}", "alpha ".repeat(20).trim());
        let blocker = render_blockers(&rule).join("\n");
        for harness in registry::all() {
            let error = render_rule_files(&rule, &harness.id).expect_err("must refuse");
            match error {
                AppError::InvalidArgument { message } => {
                    assert_eq!(message, blocker, "{}", harness.id);
                }
                other => panic!("{} returned {other:?}", harness.id),
            }
        }
    }

    // ── slug validation ────────────────────────────────────────────────────

    #[test]
    fn validate_slug_accepts_the_schema_pattern() {
        for slug in ["a", "typescript", "oh-my-pi", "a1", "a-b-c1"] {
            assert!(validate_slug(slug).is_ok(), "{slug} must be accepted");
        }
    }

    #[test]
    fn validate_slug_rejects_every_traversal_and_shape_violation() {
        for slug in [
            "",
            "A-B",
            "Upper",
            "a/b",
            "../../etc/passwd",
            "..",
            ".hidden",
            "-leading",
            "a_b",
            "a b",
            "a\\b",
            "á",
        ] {
            assert!(validate_slug(slug).is_err(), "{slug:?} must be rejected");
        }
    }

    #[test]
    fn validate_slug_mirrors_the_schema_length_cap() {
        // 64 is the schema's `name` maxLength and must be accepted.
        let ok = "a".repeat(64);
        assert!(validate_slug(&ok).is_ok(), "64 chars must be accepted");

        // One more is not, even though it matches the pattern: without this a
        // caller that skipped schema validation could reach a write with a name
        // no filesystem can hold.
        let too_long = "a".repeat(65);
        assert!(
            validate_slug(&too_long).is_err(),
            "65 chars must be rejected before any join"
        );
    }

    // ── destinations ───────────────────────────────────────────────────────

    fn temp_home() -> tempfile::TempDir {
        tempfile::tempdir().expect("tempdir")
    }

    /// A synthetic harness record, so the scope/template rejection branches can
    /// be reached without editing the bundled contract.
    fn harness_meta(id: &str, user: Option<&str>, project: Option<&str>) -> RuleHarnessMeta {
        RuleHarnessMeta {
            id: id.to_string(),
            label: id.to_string(),
            format: "md-or-mdc".to_string(),
            install_kind: "per-rule".to_string(),
            integration_path: "{slug}.md".to_string(),
            detect: None,
            frontmatter: None,
            extensions: Vec::new(),
            plugin: None,
            dest: crate::types::RuleHarnessDest {
                user: user.map(str::to_string),
                project: project.map(str::to_string),
            },
        }
    }

    #[test]
    fn every_harness_resolves_one_user_dest_under_the_given_base() {
        let home = temp_home();
        for harness in registry::all() {
            let dests = rule_dests(&harness.id, "typescript", home.path(), None)
                .unwrap_or_else(|e| panic!("{} must resolve: {e:?}", harness.id));
            assert_eq!(dests.len(), 1, "{}", harness.id);
            assert!(
                dests[0].starts_with(home.path()),
                "{} escaped home: {}",
                harness.id,
                dests[0].display()
            );
            let rendered = dests[0].to_string_lossy().to_string();
            assert!(
                !rendered.contains("{{"),
                "{} left a placeholder behind: {rendered}",
                harness.id
            );
            assert!(
                dests[0]
                    .file_name()
                    .and_then(|name| name.to_str())
                    .is_some_and(|name| name == "typescript.md" || name == "RULES.md"),
                "{} resolved an unexpected filename: {rendered}",
                harness.id
            );
        }
    }

    #[test]
    fn every_harness_resolves_one_project_dest_under_the_project_root() {
        let home = temp_home();
        let project = temp_home();
        for harness in registry::all() {
            let dests = rule_dests(&harness.id, "python", home.path(), Some(project.path()))
                .unwrap_or_else(|e| panic!("{} must resolve: {e:?}", harness.id));
            assert_eq!(dests.len(), 1, "{}", harness.id);
            assert!(
                dests[0].starts_with(project.path()),
                "{} must root at the project, not home: {}",
                harness.id,
                dests[0].display()
            );
            assert!(
                !dests[0].starts_with(home.path()),
                "{} must not write into home for project scope",
                harness.id
            );
            assert!(
                !dests[0].to_string_lossy().contains("{{"),
                "{} left a placeholder behind: {}",
                harness.id,
                dests[0].display()
            );
        }
    }

    #[test]
    fn pi_agent_nests_one_directory_and_the_others_do_not() {
        let home = temp_home();
        let pi = rule_dests("piAgent", "typescript", home.path(), None).expect("piAgent");
        assert_eq!(
            pi[0],
            home.path().join(".pi/agent/rules/typescript/RULES.md")
        );
        let pi_relative = pi[0]
            .strip_prefix(home.path())
            .expect("dest under home")
            .components()
            .count();
        for harness in ["antigravity", "ohMyPi", "opencode"] {
            let dests = rule_dests(harness, "typescript", home.path(), None).expect("resolves");
            let relative = dests[0]
                .strip_prefix(home.path())
                .expect("dest under home")
                .components()
                .count();
            assert_eq!(
                relative + 1,
                pi_relative,
                "{harness} must not add a directory level: {}",
                dests[0].display()
            );
        }
    }

    /// The exact templates from the contract, spelled out so a contract edit
    /// that silently moves a rule is caught here as well as in the fixtures.
    #[test]
    fn shipped_destinations_expand_exactly() {
        let home = Path::new("/home/tester");
        let project = Path::new("/work/project");
        let cases = [
            (
                "antigravity",
                "/home/tester/.gemini/config/rules/typescript.md",
                "/work/project/.agents/rules/typescript.md",
            ),
            (
                "piAgent",
                "/home/tester/.pi/agent/rules/typescript/RULES.md",
                "/work/project/.pi/rules/typescript/RULES.md",
            ),
            (
                "ohMyPi",
                "/home/tester/.omp/agent/rules/typescript.md",
                "/work/project/.omp/rules/typescript.md",
            ),
            (
                "opencode",
                "/home/tester/.config/opencode/rules/typescript.md",
                "/work/project/.opencode/rules/typescript.md",
            ),
        ];
        for (harness, user, project_dest) in cases {
            let resolved = rule_dests(harness, "typescript", home, None).expect("user dest");
            assert_eq!(resolved[0], PathBuf::from(user), "{harness} user");
            let resolved =
                rule_dests(harness, "typescript", home, Some(project)).expect("project dest");
            assert_eq!(
                resolved[0],
                PathBuf::from(project_dest),
                "{harness} project"
            );
        }
    }

    /// A hostile slug is rejected BEFORE any join, and touches nothing.
    #[test]
    fn hostile_slugs_are_rejected_with_zero_filesystem_contact() {
        let home = temp_home();
        for slug in ["../../etc/passwd", "a/b", "A-B", ""] {
            for harness in ["antigravity", "piAgent", "ohMyPi", "opencode"] {
                assert!(
                    rule_dests(harness, slug, home.path(), None).is_err(),
                    "{harness} accepted {slug:?}"
                );
            }
        }
        assert!(
            std::fs::read_dir(home.path())
                .expect("home readable")
                .next()
                .is_none(),
            "the resolver must not create anything"
        );
    }

    #[test]
    fn rule_dests_rejects_an_unknown_harness_without_a_default_path() {
        let home = temp_home();
        let error =
            rule_dests("not-a-harness", "typescript", home.path(), None).expect_err("must refuse");
        assert!(
            matches!(&error, AppError::InvalidArgument { message } if message == "unknown rule harness: not-a-harness"),
            "{error:?}"
        );
    }

    /// A project-only harness asked for user scope gets the agent path's
    /// wording — not a silent `~` write.
    #[test]
    fn project_only_harness_without_a_root_needs_a_project_path() {
        let home = temp_home();
        let meta = harness_meta("project-only", None, Some(".only/rules/{slug}.md"));
        let error =
            rule_dests_with(&meta, "typescript", home.path(), None).expect_err("must refuse");
        assert!(
            matches!(
                &error,
                AppError::InvalidArgument { message }
                    if message == "rule harness 'project-only' is project-scoped; a project path is required"
            ),
            "{error:?}"
        );
        assert!(
            std::fs::read_dir(home.path())
                .expect("home readable")
                .next()
                .is_none(),
            "refusal must not touch the filesystem"
        );
    }

    /// The mirror branch: a user-only harness refuses project scope rather than
    /// writing `./.user/rules/…` into an arbitrary directory.
    #[test]
    fn user_only_harness_refuses_project_scope() {
        let home = temp_home();
        let project = temp_home();
        let meta = harness_meta("user-only", Some("~/.only/rules/{slug}.md"), None);
        let error = rule_dests_with(&meta, "typescript", home.path(), Some(project.path()))
            .expect_err("must refuse");
        assert!(
            matches!(
                &error,
                AppError::InvalidArgument { message }
                    if message == "rule harness 'user-only' is user-scoped; a project path is not supported"
            ),
            "{error:?}"
        );
    }

    /// The block-scalar branch, byte-compared against the reference compiler.
    ///
    /// The fixture is generated, not written: `yaml@2`'s
    /// `stringify(...).trimEnd()` — exactly what `_util.mjs` does — for a matrix
    /// of adversarial shapes (chomping, indent indicators, blank lines,
    /// more-indented lines, folded paragraphs, overflow fallbacks, unicode).
    #[test]
    fn block_scalars_match_the_reference_compiler() {
        #[derive(serde::Deserialize)]
        struct Case {
            name: String,
            value: String,
            expected: String,
        }
        #[derive(serde::Deserialize)]
        struct Fixture {
            cases: Vec<Case>,
        }

        let raw = include_str!("../../tests/fixtures/rules/block-scalars.json");
        let fixture: Fixture = serde_json::from_str(raw).expect("the fixture parses");
        assert!(!fixture.cases.is_empty(), "the fixture carries no cases");

        let mut failures = Vec::new();
        let mut folded = 0usize;
        for case in &fixture.cases {
            let expected_folded = case.expected.starts_with("description: >");
            let mut rule = sample();
            rule.description = case.value.clone();
            let blocked = !render_blockers(&rule).is_empty();
            if expected_folded {
                // The one shape still refused: folding is not ported, so the
                // guard must catch it rather than emit different bytes.
                folded += 1;
                if !blocked {
                    failures.push(format!(
                        "{}\n  expected the folded shape to be refused",
                        case.name
                    ));
                }
                continue;
            }
            if blocked {
                failures.push(format!(
                    "{}\n  refused, but the reference renders it as a literal block",
                    case.name
                ));
                continue;
            }
            let actual = emit_block(&[("description", YamlValue::Scalar(case.value.clone()))]);
            if actual != case.expected {
                failures.push(format!(
                    "{}\n  expected: {:?}\n  actual:   {:?}",
                    case.name, case.expected, actual
                ));
            }
        }
        assert!(folded > 0, "the fixture must keep at least one folded case");
        assert!(
            failures.is_empty(),
            "{} block-scalar case(s) diverge from yaml@2:\n{}",
            failures.len(),
            failures.join("\n")
        );
    }

    /// The legacy double-brace placeholder must not be silently accepted: it
    /// resolves to a literal `{slug}` directory that no harness reads.
    #[test]
    fn double_brace_templates_are_rejected() {
        let home = temp_home();
        let mut meta = harness_meta("legacy", Some("~/.agent/rules/{{slug}}.md"), None);
        meta.integration_path = "{{slug}}.md".to_string();
        let error = rule_dests_with(&meta, "typescript", home.path(), None)
            .expect_err("must refuse the legacy double-brace placeholder");
        assert!(
            matches!(&error, AppError::InvalidArgument { message } if message.contains("{slug} placeholder")),
            "{error:?}"
        );
        assert!(
            std::fs::read_dir(home.path())
                .expect("home readable")
                .next()
                .is_none(),
            "refusal must not touch the filesystem"
        );
    }

    #[test]
    fn absolute_destination_templates_are_rejected() {
        let home = temp_home();
        let meta = harness_meta("absolute", Some("/etc/rules/{slug}.md"), None);
        assert!(rule_dests_with(&meta, "typescript", home.path(), None).is_err());
    }

    /// A template cannot climb out of its root either — belt and braces behind
    /// the slug gate, since a future contract edit could introduce one.
    #[test]
    fn dot_segments_in_a_template_are_rejected() {
        let home = temp_home();
        for template in ["a/../{slug}.md", "./{slug}.md"] {
            let meta = harness_meta("dotty", Some(template), None);
            let error = rule_dests_with(&meta, "typescript", home.path(), None)
                .expect_err("must refuse a dot segment");
            assert!(
                matches!(&error, AppError::InvalidArgument { message } if message.contains("escapes its root")),
                "{error:?}"
            );
        }
    }

    // ── harness base resolver (ruling A7, fallback C9) ─────────────────────

    #[test]
    fn harness_base_is_the_os_home_when_no_override_is_set() {
        let home = Path::new("/home/tester");
        let tool_paths = HashMap::new();
        for harness in registry::all() {
            assert_eq!(
                harness_base(&tool_paths, &harness.id, home),
                home,
                "{} must resolve to the OS home with no override",
                harness.id
            );
        }
    }

    #[test]
    fn harness_base_honours_the_configured_tool_path() {
        let home = Path::new("/home/tester");
        let tool_paths = HashMap::from([
            ("piAgent".to_string(), "/wsl/home/me".to_string()),
            ("opencode".to_string(), "/opt/opencode-home".to_string()),
        ]);
        assert_eq!(
            harness_base(&tool_paths, "piAgent", home),
            PathBuf::from("/wsl/home/me")
        );
        assert_eq!(
            harness_base(&tool_paths, "opencode", home),
            PathBuf::from("/opt/opencode-home")
        );
        // Untouched harness keeps the OS home.
        assert_eq!(harness_base(&tool_paths, "antigravity", home), home);
        // An empty override is "not set" (the `resolve_tool_base` contract).
        let empty = HashMap::from([("ohMyPi".to_string(), String::new())]);
        assert_eq!(harness_base(&empty, "ohMyPi", home), home);
    }

    /// C9: the "no registry match → OS home" branch is unreachable with the
    /// shipped contract, so exercise it synthetically or it is untested code.
    #[test]
    fn harness_base_falls_back_to_the_os_home_for_an_unknown_harness() {
        let home = Path::new("/home/tester");
        let tool_paths = HashMap::from([("piAgent".to_string(), "/wsl/home/me".to_string())]);
        assert_eq!(harness_base(&tool_paths, "not-a-harness", home), home);
    }

    /// A7 must not move an agent path: `render::dests` (agent installs) reads
    /// `registry::Dest`, never `tool_paths`, so an override only affects rules.
    #[test]
    fn a_tool_path_override_does_not_change_an_agent_destination() {
        let home = Path::new("/home/tester");
        let tool_paths = HashMap::from([("opencode".to_string(), "/wsl/home/me".to_string())]);
        let agent = crate::render::dests("opencode", "frontend-developer", home, None)
            .expect("opencode resolves a user dest");
        let rule_base = harness_base(&tool_paths, "opencode", home);
        assert_ne!(rule_base, home, "the override must apply to rules");
        assert!(
            agent.iter().all(|path| path.starts_with(home)),
            "agent dests must stay rooted at home: {agent:?}"
        );
    }
}
