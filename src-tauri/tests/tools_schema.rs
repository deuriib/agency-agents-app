//! Issue #30 — shape gate for the bundled tool registry.
//!
//! `src/registry.rs` embeds `data/tools.json` with `include_str!`, and the
//! frontend imports the same file, so a malformed entry is an authoring error
//! that reaches every consumer. `data/_schema.json` describes the contract
//! (Draft 2020-12, inline `description` on every property so IDEs autocomplete
//! and explain the fields); this test is the gate that keeps the two in sync.
//!
//! Run with `cargo test --test tools_schema`.

use serde_json::{json, Value};

const SCHEMA_JSON: &str = include_str!("../data/_schema.json");
const TOOLS_JSON: &str = include_str!("../data/tools.json");

fn schema_value() -> Value {
    serde_json::from_str(SCHEMA_JSON).expect("data/_schema.json is not valid JSON")
}

fn validator() -> jsonschema::Validator {
    jsonschema::validator_for(&schema_value())
        .expect("data/_schema.json is not a valid JSON Schema")
}

fn tools_value() -> Value {
    serde_json::from_str(TOOLS_JSON).expect("data/tools.json is not valid JSON")
}

fn errors(instance: &Value) -> Vec<String> {
    validator()
        .iter_errors(instance)
        .map(|error| format!("{error} at {}", error.instance_path()))
        .collect()
}

#[test]
fn schema_declares_draft_2020_12() {
    assert_eq!(
        schema_value()["$schema"],
        "https://json-schema.org/draft/2020-12/schema"
    );
}

#[test]
fn tools_json_validates_against_schema() {
    let found = errors(&tools_value());
    assert!(
        found.is_empty(),
        "data/tools.json violates data/_schema.json:\n{}",
        found.join("\n")
    );
}

#[test]
fn schema_rejects_a_tool_missing_a_required_field() {
    let mut tools = tools_value();
    tools["tools"]["codex"]
        .as_object_mut()
        .expect("codex entry is an object")
        .remove("installKind");

    let found = errors(&tools);
    assert!(
        found.iter().any(|message| message.contains("installKind")),
        "a tool without installKind must be rejected, got: {found:?}"
    );
}

#[test]
fn schema_rejects_malformed_values_and_unknown_keys() {
    let mut tools = tools_value();
    let codex = tools["tools"]["codex"]
        .as_object_mut()
        .expect("codex entry is an object");
    codex.insert("accent".to_string(), json!("orange"));
    codex.insert("installKind".to_string(), json!("sometimes"));
    codex.insert("mysteryField".to_string(), json!(true));

    let found = errors(&tools);
    for expected in ["accent", "installKind", "mysteryField"] {
        assert!(
            found.iter().any(|message| message.contains(expected)),
            "expected a schema error about `{expected}`, got: {found:?}"
        );
    }
}

#[test]
fn schema_allows_the_optional_schema_key_for_upstream_adoption() {
    let mut tools = tools_value();
    tools["$schema"] = json!("./_schema.json");
    assert!(
        validator().is_valid(&tools),
        "the canonical upstream manifest may carry `$schema` without failing app-side validation"
    );
}
