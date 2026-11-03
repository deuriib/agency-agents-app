//! Rule harness registry — the single source of truth for rule harnesses.
//!
//! Loaded from `data/rules-tools.json`, a verbatim copy of the upstream
//! `agency-rules/tools.json` `harnesses` object, plus `data/rule-formats.json`,
//! the list of formats this app renders natively. Mirrors `src-tauri/src/registry.rs`
//! exactly: the JSON carries *upstream truth* (id, label, format, destinations);
//! whether this app can install a harness is **derived**, not stored — a harness
//! is installable iff we ship a native renderer for its `format`.
//!
//! The frontend (`src/lib/data/ruleRegistry.ts`) imports the same two files, so
//! the two sides cannot drift. Never restate the harness or format list here
//! (spec §5.5, `registry.rs:12-14`, issue #49). Destinations use `{{slug}}`
//! (double braces) — the agent contract uses `{slug}`; do not unify them.

use std::collections::{BTreeMap, HashSet};
use std::sync::OnceLock;

use serde::Deserialize;

use crate::types::RuleHarnessMeta;

const RULES_TOOLS_JSON: &str = include_str!("../../data/rules-tools.json");
const RULE_FORMATS_JSON: &str = include_str!("../../data/rule-formats.json");

/// The rule render formats this app implements natively, read from
/// `data/rule-formats.json`.
///
/// That file is the single source of truth: the frontend imports the same JSON,
/// so the two sides cannot drift. A harness whose format is absent is
/// recognised-but-not-installable; adding a renderer means adding its format to
/// that file, never to this one.
pub fn implemented_formats() -> &'static HashSet<String> {
    static FORMATS: OnceLock<HashSet<String>> = OnceLock::new();
    FORMATS.get_or_init(|| {
        #[derive(Deserialize)]
        struct Spec {
            formats: Vec<String>,
        }
        let spec: Spec = serde_json::from_str(RULE_FORMATS_JSON)
            .unwrap_or_else(|e| panic!("invalid data/rule-formats.json: {e}"));
        spec.formats.into_iter().collect()
    })
}

/// The derived-installability predicate, kept separate from the `OnceLock` so a
/// test can inject a contract without editing bundled data.
pub fn is_installable_with(format: &str, formats: &HashSet<String>) -> bool {
    formats.contains(format)
}

/// The contract wrapper: `{ "_note": …, "version": 1, "harnesses": { … } }`.
#[derive(Deserialize)]
struct Contract {
    harnesses: BTreeMap<String, RuleHarnessMeta>,
}

/// Parse + cache the contract on first access. Panics on malformed JSON — that
/// is a build-time authoring error in the bundled contract, not a runtime
/// condition (same rationale as `registry.rs:155-161`).
fn registry() -> &'static Vec<RuleHarnessMeta> {
    static REG: OnceLock<Vec<RuleHarnessMeta>> = OnceLock::new();
    REG.get_or_init(|| {
        let contract: Contract = serde_json::from_str(RULES_TOOLS_JSON)
            .unwrap_or_else(|e| panic!("invalid data/rules-tools.json: {e}"));
        let mut harnesses: Vec<RuleHarnessMeta> = contract.harnesses.into_values().collect();
        // Deterministic order for snapshots: label, then id.
        harnesses.sort_by(|a, b| a.label.cmp(&b.label).then_with(|| a.id.cmp(&b.id)));
        harnesses
    })
}

/// All harnesses the contract declares, ordered by `(label, id)`.
///
/// Determinism matters only for snapshot-shaped tests; the spec mandates no
/// particular order.
pub fn all() -> &'static [RuleHarnessMeta] {
    registry().as_slice()
}

/// Look up a harness by its contract id, e.g. `"oh-my-pi"`.
pub fn get(id: &str) -> Option<&'static RuleHarnessMeta> {
    registry().iter().find(|h| h.id == id)
}

/// Iterator over the harnesses THIS app can install (native renderer exists).
pub fn installable() -> impl Iterator<Item = &'static RuleHarnessMeta> {
    registry().iter().filter(|h| h.installable())
}

/// The `plugin.entry` value the OpenCode config must register, from the
/// contract (ruling A6: never hardcode it). `None` for a harness with no plugin
/// block — including the three non-OpenCode harnesses.
pub fn plugin_entry(id: &str) -> Option<&'static str> {
    get(id)?.plugin.as_ref().map(|plugin| plugin.entry.as_str())
}

// =========================================================
// Tests
// =========================================================

#[cfg(test)]
mod tests {
    use super::*;

    /// A harness contract with one known and one future (unimplemented) format.
    /// Literal-in-test injection is the only way to exercise the derived
    /// predicate without editing the bundled file.
    const INJECTED: &str = r#"{
      "harnesses": {
        "antigravity": {
          "id": "antigravity",
          "label": "Google Antigravity",
          "format": "md-yaml-trigger",
          "installKind": "per-rule",
          "integrationPath": "{{slug}}.md",
          "dest": {
            "project": ".agents/rules/{{slug}}.md",
            "user": "~/.gemini/config/rules/{{slug}}.md"
          }
        },
        "future-harness": {
          "id": "future-harness",
          "label": "Future Harness",
          "format": "md-future-format",
          "installKind": "per-rule",
          "integrationPath": "{{slug}}.md",
          "dest": { "project": ".future/rules/{{slug}}.md" }
        }
      }
    }"#;

    fn injected() -> Contract {
        serde_json::from_str(INJECTED).expect("the injected contract parses")
    }

    #[test]
    fn contract_loads_exactly_four_harnesses() {
        assert_eq!(all().len(), 4, "expected the four bundled rule harnesses");
        let mut ids: Vec<&str> = all().iter().map(|h| h.id.as_str()).collect();
        ids.sort_unstable();
        assert_eq!(ids, vec!["antigravity", "oh-my-pi", "opencode", "pi-agent"]);
    }

    #[test]
    fn harnesses_are_ordered_by_label_then_id() {
        let sorted: Vec<(&str, &str)> = all()
            .iter()
            .map(|h| (h.label.as_str(), h.id.as_str()))
            .collect();
        let mut expected = sorted.clone();
        expected.sort_unstable();
        assert_eq!(sorted, expected, "registry order must be deterministic");
    }

    #[test]
    fn every_harness_format_resolves() {
        let formats = implemented_formats();
        assert!(
            !formats.is_empty(),
            "data/rule-formats.json lists no formats — every harness would be uninstallable"
        );
        for harness in all() {
            assert!(
                formats.contains(&harness.format),
                "harness '{}' declares format '{}' which data/rule-formats.json does not list",
                harness.id,
                harness.format
            );
            assert!(
                harness.installable(),
                "harness '{}' should be installable",
                harness.id
            );
            assert_eq!(installable().filter(|h| h.id == harness.id).count(), 1);
        }
    }

    /// Covers the four shipped mappings and proves the format decides
    /// installability: a harness injected with an unknown format is recognised
    /// but never offered.
    #[test]
    fn a_harness_with_an_unimplemented_format_is_not_installable() {
        let contract = injected();
        let future = contract
            .harnesses
            .get("future-harness")
            .expect("injected harness present");

        assert!(
            !is_installable_with(&future.format, implemented_formats()),
            "an unknown format must not be installable"
        );
        assert!(
            !future.installable(),
            "installable() must be derived from data/rule-formats.json, not hardcoded"
        );

        let known = contract
            .harnesses
            .get("antigravity")
            .expect("injected known harness present");
        assert!(is_installable_with(&known.format, implemented_formats()));
        assert!(known.installable());
    }

    #[test]
    fn unknown_harness_is_rejected() {
        assert!(get("not-a-harness").is_none());
        assert!(plugin_entry("not-a-harness").is_none());
    }

    #[test]
    fn support_capabilities_are_derived_from_destinations() {
        for harness in all() {
            assert!(harness.supports_user(), "{} needs a user dest", harness.id);
            assert!(
                harness.supports_project(),
                "{} needs a project dest",
                harness.id
            );
        }

        // Project-only harness: user scope is derived off, not stored.
        let project_only = injected();
        let future: &RuleHarnessMeta = project_only
            .harnesses
            .get("future-harness")
            .expect("injected harness present");
        assert!(!future.supports_user());
        assert!(future.supports_project());
    }

    #[test]
    fn plugin_entry_comes_from_the_contract() {
        assert_eq!(plugin_entry("opencode"), Some("opencode-rules@latest"));
        assert!(plugin_entry("antigravity").is_none());
        assert!(plugin_entry("pi-agent").is_none());
        assert!(plugin_entry("oh-my-pi").is_none());
    }

    #[test]
    fn every_harness_has_the_fields_a_renderer_and_dest_resolver_need() {
        for harness in all() {
            assert!(!harness.id.is_empty());
            assert!(!harness.label.is_empty());
            assert_eq!(
                harness.install_kind, "per-rule",
                "{} installs one file per rule",
                harness.id
            );
            assert!(
                harness.integration_path.contains("{{slug}}"),
                "{} integrationPath must use {{{{slug}}}}",
                harness.id
            );
            for template in [
                harness.dest.user.as_deref(),
                harness.dest.project.as_deref(),
            ]
            .into_iter()
            .flatten()
            {
                assert!(
                    template.contains("{{slug}}"),
                    "{} destination template must use {{{{slug}}}}: {template}",
                    harness.id
                );
                // `{{slug}}` itself contains `{slug}`, so strip the double-brace
                // placeholder first and assert nothing brace-like survives.
                let residual = template.replace("{{slug}}", "");
                assert!(
                    !residual.contains('{'),
                    "{} destination template must not use the agent placeholder: {template}",
                    harness.id
                );
            }
        }
    }

    /// The guard from `registry.rs:242-260`, applied to the rule contract: the
    /// two `include_str!` constants are the only source of harness/format data.
    /// No format id and no destination path may appear in production code.
    #[test]
    fn contracts_are_single_sourced_and_resolvable() {
        let formats = implemented_formats();

        // Every format the file lists is declared by at least one harness — an
        // orphan is a typo or a leftover and silently does nothing.
        let declared: std::collections::BTreeSet<&str> =
            all().iter().map(|h| h.format.as_str()).collect();
        for format in formats {
            assert!(
                declared.contains(format.as_str()),
                "rule format '{format}' in data/rule-formats.json matches no harness in data/rules-tools.json"
            );
        }

        // Both contracts are embedded, never restated.
        const SELF_SOURCE: &str = include_str!("registry.rs");
        let production: String = SELF_SOURCE
            .split("#[cfg(test)]")
            .next()
            .unwrap_or(SELF_SOURCE)
            .lines()
            // Doc/line comments may name a format to explain it; code may not.
            .filter(|line| {
                let trimmed = line.trim_start();
                !(trimmed.starts_with("//")
                    || trimmed.starts_with("/*")
                    || trimmed.starts_with('*'))
            })
            .collect::<Vec<_>>()
            .join("\n");

        for needle in [
            r#"include_str!("../../data/rules-tools.json")"#,
            r#"include_str!("../../data/rule-formats.json")"#,
        ] {
            assert!(
                production.contains(needle),
                "rules/registry.rs must embed {needle}"
            );
        }

        for format in formats {
            assert!(
                !production.contains(format.as_str()),
                "format '{format}' is hardcoded in rules/registry.rs — read it from data/rule-formats.json"
            );
        }

        for harness in all() {
            for template in [
                harness.dest.user.as_deref(),
                harness.dest.project.as_deref(),
            ]
            .into_iter()
            .flatten()
            {
                assert!(
                    !production.contains(template),
                    "destination '{template}' is hardcoded in rules/registry.rs — read it from data/rules-tools.json"
                );
            }
        }
    }
}
