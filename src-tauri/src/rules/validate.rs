//! Fail-closed frontmatter validation against the shipped JSON Schema.
//!
//! `data/rules-schema.json` is a byte-identical copy of `agency-rules/rules/_schema.json`
//! (draft-07, `additionalProperties: false`, `required: ["name", "description"]`).
//! A rule is validated **before** any compile or write, so a structurally invalid
//! rule is rejected with the schema's own message and nothing touches disk
//! (spec §8: "surfaced verbatim").
//!
//! Pure: no filesystem, no clock, no I/O. The caller (`rules/parse.rs`) owns the
//! bytes; this module only sees the frontmatter document.

use std::sync::OnceLock;

use serde_json::Value;

use crate::types::RuleValidation;

const RULES_SCHEMA_JSON: &str = include_str!("../../data/rules-schema.json");

/// The compiled schema, built once. A malformed bundled schema is a build-time
/// authoring error, not a runtime condition (same rationale as
/// `registry.rs:155-161`).
fn validator() -> &'static jsonschema::Validator {
    static VALIDATOR: OnceLock<jsonschema::Validator> = OnceLock::new();
    VALIDATOR.get_or_init(|| {
        let schema: Value = serde_json::from_str(RULES_SCHEMA_JSON)
            .unwrap_or_else(|e| panic!("invalid data/rules-schema.json: {e}"));
        jsonschema::validator_for(&schema)
            .unwrap_or_else(|e| panic!("data/rules-schema.json is not a valid JSON Schema: {e}"))
    })
}

/// Validate a rule's frontmatter document.
///
/// `errors` are the validator's own messages, unprefixed and untruncated —
/// callers must surface them verbatim, never re-word or summarize them.
pub fn validate_frontmatter(frontmatter: &Value) -> RuleValidation {
    let errors: Vec<String> = validator()
        .iter_errors(frontmatter)
        .map(|error| error.to_string())
        .collect();
    RuleValidation {
        valid: errors.is_empty(),
        errors,
    }
}

// =========================================================
// Tests
// =========================================================

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn errors_for(frontmatter: Value) -> Vec<String> {
        validate_frontmatter(&frontmatter).errors
    }

    fn assert_invalid(frontmatter: Value, needle: &str) {
        let validation = validate_frontmatter(&frontmatter);
        assert!(!validation.valid, "expected {frontmatter} to fail");
        assert!(
            validation.errors.iter().any(|e| e.contains(needle)),
            "expected an error mentioning {needle:?}, got {:?}",
            validation.errors
        );
    }

    #[test]
    fn a_valid_rule_passes() {
        let validation = validate_frontmatter(&json!({
            "name": "typescript",
            "description": "Mandatory rules for editing TypeScript and TSX files.",
            "globs": ["**/*.ts", "**/*.tsx"],
            "alwaysApply": false,
            "trigger": "glob",
            "match": "any",
            "tags": ["language", "typescript"],
            "tools": ["read_file"]
        }));
        assert!(
            validation.valid,
            "unexpected errors: {:?}",
            validation.errors
        );
        assert!(validation.errors.is_empty());
    }

    /// The corpus-real shape: `rules/stacks/typescript.md`'s frontmatter.
    #[test]
    fn a_corpus_real_rule_validates_clean() {
        let validation = validate_frontmatter(&json!({
            "name": "typescript",
            "description": "Mandatory rules for editing TypeScript and TSX files.",
            "globs": ["**/*.ts", "**/*.tsx"],
            "alwaysApply": false,
            "trigger": "glob",
            "tags": ["language", "typescript"]
        }));
        assert!(
            validation.valid,
            "unexpected errors: {:?}",
            validation.errors
        );
    }

    /// Spec §5.1: only `description` is required beyond `name`.
    #[test]
    fn a_description_only_rule_is_valid() {
        let validation = validate_frontmatter(&json!({
            "name": "api-review",
            "description": "Use this rule when reviewing REST API endpoint definitions."
        }));
        assert!(
            validation.valid,
            "unexpected errors: {:?}",
            validation.errors
        );
    }

    #[test]
    fn a_rule_with_only_description_is_still_missing_name() {
        assert_invalid(json!({ "description": "Only a description." }), "name");
    }

    #[test]
    fn a_rule_missing_description_fails() {
        assert_invalid(json!({ "name": "typescript" }), "description");
    }

    #[test]
    fn a_rule_missing_name_fails() {
        assert_invalid(json!({ "description": "No name." }), "name");
    }

    #[test]
    fn a_name_violating_the_pattern_fails() {
        for bad in [
            "TypeScript",
            "has space",
            "1leading",
            "-leading",
            "trailing_",
        ] {
            assert_invalid(json!({ "name": bad, "description": "d" }), "does not match");
        }
    }

    #[test]
    fn a_name_longer_than_64_chars_fails() {
        let name = "a".repeat(65);
        let validation = validate_frontmatter(&json!({ "name": name, "description": "d" }));
        assert!(!validation.valid);
        assert!(
            validation.errors.iter().any(|e| e.contains("64")),
            "expected a maxLength error, got {:?}",
            validation.errors
        );
    }

    #[test]
    fn an_empty_description_fails() {
        assert_invalid(json!({ "name": "ok", "description": "" }), "shorter than 1");
    }

    #[test]
    fn a_description_longer_than_1024_chars_fails() {
        let description = "a".repeat(1025);
        let validation = validate_frontmatter(&json!({ "name": "ok", "description": description }));
        assert!(!validation.valid);
        assert!(
            validation.errors.iter().any(|e| e.contains("1024")),
            "expected a maxLength error, got {:?}",
            validation.errors
        );
    }

    /// `additionalProperties: false` is the guard against a silently mistyped
    /// key — `alwayApply` must fail loudly instead of being ignored.
    #[test]
    fn an_unknown_key_fails() {
        assert_invalid(
            json!({ "name": "ok", "description": "d", "alwayApply": true }),
            "alwayApply",
        );
    }

    #[test]
    fn enums_and_shapes_are_enforced() {
        assert_invalid(
            json!({ "name": "ok", "description": "d", "trigger": "sometimes" }),
            "sometimes",
        );
        assert_invalid(
            json!({ "name": "ok", "description": "d", "match": "some" }),
            "some",
        );
        assert_invalid(
            json!({ "name": "ok", "description": "d", "globs": [] }),
            "oneOf",
        );
        assert_invalid(
            json!({ "name": "ok", "description": "d", "alwaysApply": "yes" }),
            "boolean",
        );
        assert_invalid(
            json!({ "name": "ok", "description": "d", "tags": ["ok", 7] }),
            "string",
        );
    }

    /// Purity guard (task 4: "no filesystem access in validate.rs"). Production
    /// code must not reference an fs API; the schema is compiled from the
    /// embedded string.
    #[test]
    fn validation_has_no_filesystem_access() {
        const SELF_SOURCE: &str = include_str!("validate.rs");
        let production: String = SELF_SOURCE
            .split("#[cfg(test)]")
            .next()
            .unwrap_or(SELF_SOURCE)
            .lines()
            .filter(|line| !line.trim_start().starts_with("//"))
            .collect::<Vec<_>>()
            .join("\n");
        for forbidden in ["std::fs", "tokio::fs", "File::open", "write("] {
            assert!(
                !production.contains(forbidden),
                "rules/validate.rs must stay pure; found {forbidden:?} in production code"
            );
        }
    }
}
