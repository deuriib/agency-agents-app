//! Parse a neutral rule (`rules/<category>/<file>.md`) into a typed [`Rule`].
//!
//! The neutral format is the only authored format: YAML frontmatter fenced by
//! `---`, then a Markdown body copied verbatim into every compiled variant. This
//! module splits those, normalizes the body exactly as the renderers consume it,
//! and computes the two hashes the ledger needs (`source_hash` for the file,
//! `body_hash` for the bytes that get written) so M2 can classify an update as
//! cosmetic or substantive without a second parse.
//!
//! Pure: bytes in, `Rule` out. The directory walk lives in `rules/corpus.rs`
//! (TASK 11); this module never touches disk.
//!
//! `slug` is the frontmatter `name`, **never** the filename — the corpus proves
//! it (`rules/base/commit-convention.md` declares `name: commit-conventions`,
//! and every compiled artifact is named after the frontmatter value).

use std::path::Path;

use serde_json::Value;

use crate::error::AppError;
use crate::render::sha256_hex;
use crate::types::{Rule, RuleGlobs};

/// Whether a corpus path should be parsed as a rule: a `.md` file with no path
/// segment starting with `_` (notably `rules/_schema.json`, and any `_draft.md`).
///
/// The directory scanner (TASK 11) filters with this before reading bytes, so a
/// scratch file can never surface as an installable rule.
pub fn is_rule_file(relative_path: &str) -> bool {
    let path = Path::new(relative_path);
    if path.extension().and_then(|extension| extension.to_str()) != Some("md") {
        return false;
    }
    !path.components().any(|component| {
        matches!(component, std::path::Component::Normal(name)
            if name.to_string_lossy().starts_with('_'))
    })
}

/// Parse one rule file's bytes.
///
/// Errors (never panics):
/// - non-UTF-8 bytes → `AppError::Io` naming the path (an unreadable rule file);
/// - no frontmatter fence → `AppError::InvalidArgument`;
/// - malformed YAML, or a frontmatter that is not a mapping →
///   `AppError::InvalidArgument` carrying the parser's message;
/// - no `name` → `AppError::InvalidArgument` (the slug would be undefined, so
///   the rule cannot be keyed or listed at all).
///
/// Everything else is best-effort: a field with the wrong type is dropped from
/// the typed view, and the schema gate reports the truth in
/// `validation.errors`. That split is deliberate — a rule with
/// `alwaysApply: "yes"` must still be **listed** with its error so the UI can
/// explain why install is refused (spec §10.4), which a hard parse failure would
/// prevent.
pub fn parse_rule(relative_path: &str, bytes: &[u8]) -> Result<Rule, AppError> {
    let source = std::str::from_utf8(bytes).map_err(|_| AppError::Io {
        message: format!("read {relative_path}: not valid UTF-8"),
    })?;

    let (frontmatter_text, body) =
        split_frontmatter(source).ok_or_else(|| AppError::InvalidArgument {
            message: format!("{relative_path}: missing YAML frontmatter"),
        })?;

    let frontmatter: Value =
        serde_yaml::from_str(frontmatter_text).map_err(|e| AppError::InvalidArgument {
            message: format!("{relative_path}: invalid frontmatter: {e}"),
        })?;
    if !frontmatter.is_object() {
        return Err(AppError::InvalidArgument {
            message: format!("{relative_path}: frontmatter must be a YAML mapping"),
        });
    }

    let name = text(&frontmatter, "name").ok_or_else(|| AppError::InvalidArgument {
        message: format!("{relative_path}: frontmatter has no `name`"),
    })?;
    if name.is_empty() {
        return Err(AppError::InvalidArgument {
            message: format!("{relative_path}: frontmatter has no `name`"),
        });
    }

    // Trim here once: the renderers trim too, and `body_hash` must match the
    // bytes that get written. CRLF is normalized to LF first — the agent
    // pipeline (`render::source_body`) already emits LF-only bodies, and a
    // platform-independent body keeps `rendered_hash` stable across checkouts.
    let body = body.replace("\r\n", "\n");
    let body = body.trim();

    Ok(Rule {
        slug: name.clone(),
        name,
        description: text(&frontmatter, "description").unwrap_or_default(),
        category: category_of(relative_path),
        relative_path: relative_path.to_string(),
        globs: frontmatter
            .get("globs")
            .and_then(|value| serde_json::from_value::<RuleGlobs>(value.clone()).ok()),
        always_apply: frontmatter.get("alwaysApply").and_then(Value::as_bool),
        trigger: text(&frontmatter, "trigger"),
        match_: text(&frontmatter, "match"),
        tags: string_list(&frontmatter, "tags"),
        tools: string_list(&frontmatter, "tools"),
        body: body.to_string(),
        source_hash: sha256_hex(source.as_bytes()),
        body_hash: sha256_hex(body.as_bytes()),
        validation: crate::rules::validate::validate_frontmatter(&frontmatter),
    })
}

/// A string field, or `None` when absent or not a string. The schema gate owns
/// the type error.
fn text(frontmatter: &Value, key: &str) -> Option<String> {
    frontmatter
        .get(key)
        .and_then(Value::as_str)
        .map(str::to_string)
}

/// A `string[]` field; non-string items are dropped here and reported by the
/// schema gate.
fn string_list(frontmatter: &Value, key: &str) -> Vec<String> {
    frontmatter
        .get(key)
        .and_then(Value::as_array)
        .map(|items| {
            items
                .iter()
                .filter_map(Value::as_str)
                .map(str::to_string)
                .collect()
        })
        .unwrap_or_default()
}

/// The parent directory name, e.g. `"base"` for `rules/base/commit-convention.md`.
fn category_of(relative_path: &str) -> String {
    Path::new(relative_path)
        .parent()
        .and_then(Path::file_name)
        .and_then(|name| name.to_str())
        .unwrap_or_default()
        .to_string()
}

/// Split `---\n<frontmatter>\n---\n<body>` into `(frontmatter, body)`.
///
/// The opening fence must be the first line (YAML frontmatter convention). Only
/// the first two `---` lines are fences: later `---` lines are Markdown
/// horizontal rules and belong to the body.
fn split_frontmatter(source: &str) -> Option<(&str, &str)> {
    let (first, mut rest) = split_line(source);
    if first != "---" {
        return None;
    }
    let frontmatter_start = source.len() - rest.len();
    loop {
        if rest.is_empty() {
            return None; // unterminated frontmatter
        }
        let (line, next) = split_line(rest);
        let line_start = source.len() - rest.len();
        if line == "---" {
            return Some((&source[frontmatter_start..line_start], next));
        }
        rest = next;
    }
}

/// One line without its terminator, plus the remainder. Handles CRLF.
fn split_line(text: &str) -> (&str, &str) {
    match text.find('\n') {
        Some(index) => {
            let line = text[..index].strip_suffix('\r').unwrap_or(&text[..index]);
            (line, &text[index + 1..])
        }
        None => (text.strip_suffix('\r').unwrap_or(text), ""),
    }
}

// =========================================================
// Tests
// =========================================================

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::RuleGlobs;

    const MINIMAL: &str = "---\nname: coding-standards\ndescription: Baseline rules applied to every edit.\n---\n\n# Coding Standards\n\n- Keep functions short.\n";

    fn rule(source: &str) -> Rule {
        parse_rule("rules/base/coding-standards.md", source.as_bytes()).expect("rule parses")
    }

    #[test]
    fn parses_a_minimal_rule() {
        let parsed = rule(MINIMAL);
        assert_eq!(parsed.slug, "coding-standards");
        assert_eq!(parsed.name, "coding-standards");
        assert_eq!(parsed.description, "Baseline rules applied to every edit.");
        assert_eq!(parsed.category, "base");
        assert_eq!(parsed.relative_path, "rules/base/coding-standards.md");
        assert_eq!(parsed.body, "# Coding Standards\n\n- Keep functions short.");
        assert_eq!(parsed.globs, None);
        assert_eq!(parsed.always_apply, None);
        assert_eq!(parsed.trigger, None);
        assert_eq!(parsed.match_, None);
        assert!(parsed.tags.is_empty());
        assert!(parsed.tools.is_empty());
        assert!(parsed.validation.valid);
        assert!(parsed.validation.errors.is_empty());
    }

    #[test]
    fn category_is_the_parent_directory() {
        let parsed = parse_rule(
            "rules/stacks/typescript.md",
            MINIMAL.replace("coding-standards", "typescript").as_bytes(),
        )
        .expect("rule parses");
        assert_eq!(parsed.category, "stacks");
    }

    #[test]
    fn parses_globs_authored_as_a_string() {
        let parsed = rule(&MINIMAL.replace(
            "description: Baseline rules applied to every edit.",
            "description: d\nglobs: \"**/*.md\"",
        ));
        assert_eq!(parsed.globs, Some(RuleGlobs::One("**/*.md".into())));
    }

    #[test]
    fn parses_globs_authored_as_an_array() {
        let parsed = rule(&MINIMAL.replace(
            "description: Baseline rules applied to every edit.",
            "description: d\nglobs:\n  - \"**/*.ts\"\n  - \"**/*.tsx\"",
        ));
        assert_eq!(
            parsed.globs,
            Some(RuleGlobs::Many(vec!["**/*.ts".into(), "**/*.tsx".into()]))
        );
    }

    /// C7: `false` and absent are different rules.
    #[test]
    fn always_apply_false_and_absent_are_distinguishable() {
        let with_false = rule(&MINIMAL.replace(
            "description: Baseline rules applied to every edit.",
            "description: d\nalwaysApply: false",
        ));
        assert_eq!(with_false.always_apply, Some(false));

        let with_true = rule(&MINIMAL.replace(
            "description: Baseline rules applied to every edit.",
            "description: d\nalwaysApply: true",
        ));
        assert_eq!(with_true.always_apply, Some(true));

        let absent = rule(MINIMAL);
        assert_eq!(absent.always_apply, None);
        assert_ne!(absent.always_apply, with_false.always_apply);
    }

    /// The corpus ships this exact shape in `rules/stacks/typescript.md`.
    #[test]
    fn a_trigger_with_a_trailing_comment_is_parsed_without_the_comment() {
        let parsed = rule(&MINIMAL.replace(
            "description: Baseline rules applied to every edit.",
            "description: d\ntrigger: glob          # preferencia para Antigravity",
        ));
        assert_eq!(parsed.trigger.as_deref(), Some("glob"));
    }

    /// `rules/base/commit-convention.md` declares `name: commit-conventions`.
    #[test]
    fn slug_is_the_frontmatter_name_not_the_filename() {
        let parsed = parse_rule(
            "rules/base/commit-convention.md",
            MINIMAL
                .replace("coding-standards", "commit-conventions")
                .as_bytes(),
        )
        .expect("rule parses");
        assert_eq!(parsed.slug, "commit-conventions");
        assert_ne!(parsed.slug, "commit-convention");
        assert_eq!(parsed.relative_path, "rules/base/commit-convention.md");
    }

    #[test]
    fn the_match_field_survives_the_rust_keyword_rename() {
        let parsed = rule(&MINIMAL.replace(
            "description: Baseline rules applied to every edit.",
            "description: d\nmatch: all",
        ));
        assert_eq!(parsed.match_.as_deref(), Some("all"));
    }

    #[test]
    fn hashes_are_stable_across_repeated_parses_and_match_the_written_bytes() {
        let first = rule(MINIMAL);
        let second = rule(MINIMAL);
        assert_eq!(first.source_hash, second.source_hash);
        assert_eq!(first.body_hash, second.body_hash);

        assert_eq!(first.source_hash, sha256_hex(MINIMAL.as_bytes()));
        assert_eq!(first.body_hash, sha256_hex(first.body.as_bytes()));
        // The body is trimmed, so its hash is not the raw remainder's hash.
        assert_ne!(
            first.body_hash,
            sha256_hex(b"\n\n# Coding Standards\n\n- Keep functions short.\n")
        );
    }

    #[test]
    fn crlf_frontmatter_parses() {
        let crlf = MINIMAL.replace('\n', "\r\n");
        let parsed = rule(&crlf);
        assert_eq!(parsed.slug, "coding-standards");
        assert_eq!(parsed.body, "# Coding Standards\n\n- Keep functions short.");
    }

    #[test]
    fn missing_frontmatter_is_rejected() {
        let error = parse_rule("rules/base/plain.md", b"# Just markdown\n").expect_err("must fail");
        match error {
            AppError::InvalidArgument { message } => {
                assert!(message.contains("missing YAML frontmatter"), "{message}")
            }
            other => panic!("expected InvalidArgument, got {other:?}"),
        }
    }

    #[test]
    fn an_unterminated_frontmatter_is_rejected() {
        let error = parse_rule("rules/base/half.md", b"---\nname: half\n").expect_err("must fail");
        assert!(matches!(error, AppError::InvalidArgument { .. }));
    }

    #[test]
    fn an_empty_file_is_rejected() {
        let error = parse_rule("rules/base/empty.md", b"").expect_err("must fail");
        assert!(matches!(error, AppError::InvalidArgument { .. }));
    }

    #[test]
    fn non_utf8_bytes_are_rejected() {
        let error = parse_rule("rules/base/binary.md", &[0xff, 0xfe, 0x00]).expect_err("must fail");
        match error {
            AppError::Io { message } => {
                assert!(message.contains("rules/base/binary.md"), "{message}");
                assert!(message.contains("UTF-8"), "{message}");
            }
            other => panic!("expected Io, got {other:?}"),
        }
    }

    #[test]
    fn a_rule_without_a_name_is_rejected() {
        let error = parse_rule(
            "rules/base/no-name.md",
            b"---\ndescription: no name\n---\nbody\n",
        )
        .expect_err("must fail");
        match error {
            AppError::InvalidArgument { message } => assert!(message.contains("name"), "{message}"),
            other => panic!("expected InvalidArgument, got {other:?}"),
        }
    }

    /// A wrong-typed field is dropped from the typed view, but the rule is still
    /// listed with the schema's own error — install refuses it, list shows it.
    #[test]
    fn a_wrong_typed_field_is_retained_as_invalid() {
        let parsed = parse_rule(
            "rules/base/bad-type.md",
            b"---\nname: ok\ndescription: d\nalwaysApply: sometimes\n---\nbody\n",
        )
        .expect("parse succeeds; validation does not");
        assert_eq!(parsed.always_apply, None);
        assert!(!parsed.validation.valid);
        assert!(
            parsed
                .validation
                .errors
                .iter()
                .any(|error| error.contains("boolean")),
            "{:?}",
            parsed.validation.errors
        );
    }

    /// Structurally invalid rules must still parse — the corpus retains them so
    /// the UI can list them and show why install is refused.
    #[test]
    fn invalid_rules_still_parse_with_their_errors() {
        let parsed = parse_rule(
            "rules/base/bad-name.md",
            b"---\nname: Bad Name\ndescription: d\n---\nbody\n",
        )
        .expect("parse succeeds; validation does not");
        assert!(!parsed.validation.valid);
        assert!(
            parsed
                .validation
                .errors
                .iter()
                .any(|error| error.contains("does not match")),
            "{:?}",
            parsed.validation.errors
        );
    }

    #[test]
    fn underscore_prefixed_entries_are_not_rules() {
        assert!(!is_rule_file("rules/_schema.json"));
        assert!(!is_rule_file("rules/base/_draft.md"));
        assert!(!is_rule_file("_scratch/rule.md"));
        assert!(!is_rule_file("rules/base/notes.txt"));
        assert!(is_rule_file("rules/base/coding-standards.md"));
        assert!(is_rule_file("rules/stacks/typescript.md"));
    }

    /// Purity guard: parsing must not reach the filesystem (TASK 11 owns I/O).
    #[test]
    fn parsing_has_no_filesystem_access() {
        const SELF_SOURCE: &str = include_str!("parse.rs");
        let production: String = SELF_SOURCE
            .split("#[cfg(test)]")
            .next()
            .unwrap_or(SELF_SOURCE)
            .lines()
            .filter(|line| !line.trim_start().starts_with("//"))
            .collect::<Vec<_>>()
            .join("\n");
        for forbidden in ["std::fs", "tokio::fs", "File::open", "read_to_string"] {
            assert!(
                !production.contains(forbidden),
                "rules/parse.rs must stay pure; found {forbidden:?} in production code"
            );
        }
    }
}
