//! Golden-fixture conformance for the four native rule renderers.
//!
//! Each source rule under `tests/fixtures/rules/sources/` is parsed with the
//! real parser and rendered for every harness in the declarative contract; the
//! result must equal the vendored reference artifact **byte for byte**, including
//! the trailing newline and the blank line after the closing `---`.
//!
//! ## Provenance (ruling C5, revised)
//!
//! The artifacts are NOT copied from the sibling repo's `integrations/` tree —
//! that tree is stale (`agentmemory` drifted, `mcp-gateway-protocol` was never
//! regenerated). They come from a fresh reference run:
//!
//! ```text
//! cd /home/deuriib/projects/agency-rules
//! INTEGRATIONS_DIR=/tmp/rules-oracle node bin/agency-rules.mjs convert
//! ```
//!
//! at commit `bde758c3f6877e3f75a26eda345051a6cdba2860`. See
//! `tests/fixtures/rules/PROVENANCE.md` for the full record and the re-vendor
//! commands. Never edit a fixture to make this test pass — a mismatch is a
//! renderer bug.
//!
//! ## Coverage gap (ruling A10)
//!
//! No rule in the corpus authors `tools` or `match`, so the fixtures cannot
//! exercise them. Those two fields are covered by the renderer unit tests in
//! `src/rules/render.rs` only; this file is not exhaustive.
//!
//! ## Staleness guard (ruling A14)
//!
//! `rules_parity_against_checkout` is `#[ignore]`d and env-gated on
//! `AGENCY_RULES_PARITY_ROOT`. It re-renders from a live `agency-rules` checkout
//! and diffs that checkout's `integrations/`. A rule present upstream with no
//! compiled artifact is REPORTED, not failed, so a stale upstream tree is never
//! mistaken for a renderer bug.

use std::fs;
use std::path::{Path, PathBuf};

use agency_agents_lib::rules::parse::parse_rule;
use agency_agents_lib::rules::registry;
use agency_agents_lib::rules::render::render_rule_files;

/// The number of source rules and harnesses the fixtures must cover.
const SOURCE_COUNT: usize = 8;
const COMPARISON_COUNT: usize = SOURCE_COUNT * 4;

fn fixtures_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/rules")
}

/// Every vendored source rule as `(file name, bytes)`, sorted by file name so a
/// failure reports the same rule every run.
fn sources() -> Vec<(String, Vec<u8>)> {
    let dir = fixtures_root().join("sources");
    let mut paths: Vec<PathBuf> = fs::read_dir(&dir)
        .unwrap_or_else(|e| panic!("read {}: {e}", dir.display()))
        .map(|entry| entry.expect("readable fixture dir entry").path())
        .filter(|path| path.extension().and_then(|extension| extension.to_str()) == Some("md"))
        .collect();
    paths.sort();
    paths
        .into_iter()
        .map(|path| {
            let name = path
                .file_name()
                .and_then(|name| name.to_str())
                .expect("fixture names are UTF-8")
                .to_string();
            let bytes = fs::read(&path).unwrap_or_else(|e| panic!("read {}: {e}", path.display()));
            (name, bytes)
        })
        .collect()
}

/// Where the reference artifact for `(harness, slug)` lives, derived from the
/// contract's `integrationPath` — the same template the installer writes, minus
/// the destination root.
fn fixture_path(integration_path: &str, harness_id: &str, slug: &str) -> PathBuf {
    fixtures_root()
        .join(harness_id)
        .join(integration_path.replace("{{slug}}", slug))
}

/// The first differing byte, or `None` when the two byte strings are equal.
///
/// Compares `Vec<u8>` (not `String`) so a trailing-newline or non-UTF-8-level
/// difference cannot be normalized away.
fn first_divergence(expected: &[u8], actual: &[u8]) -> Option<usize> {
    let shared = expected.len().min(actual.len());
    for index in 0..shared {
        if expected[index] != actual[index] {
            return Some(index);
        }
    }
    if expected.len() != actual.len() {
        Some(shared)
    } else {
        None
    }
}

/// ±16 bytes around `offset`, with control characters escaped so a missing
/// newline is visible instead of invisible.
fn window(bytes: &[u8], offset: usize) -> String {
    let start = offset.saturating_sub(16);
    let end = (offset + 17).min(bytes.len());
    let mut rendered = String::new();
    for byte in &bytes[start..end] {
        match byte {
            b'\n' => rendered.push_str("\\n"),
            b'\r' => rendered.push_str("\\r"),
            b'\t' => rendered.push_str("\\t"),
            0x20..=0x7e => rendered.push(*byte as char),
            other => rendered.push_str(&format!("\\x{other:02x}")),
        }
    }
    format!("bytes[{start}..{end}] {rendered}")
}

/// A `diff`-style report naming the first divergence, its offset, the line it
/// falls on, and the surrounding bytes of both sides.
fn divergence_report(context: &str, expected: &[u8], actual: &[u8], offset: usize) -> String {
    let line = expected[..offset]
        .iter()
        .filter(|byte| **byte == b'\n')
        .count()
        + 1;
    format!(
        "{context}\n\
         first divergence at byte offset {offset} (line {line} of the expected file); \
         expected {} bytes, actual {} bytes\n\
         expected: {}\n\
         actual:   {}",
        expected.len(),
        actual.len(),
        window(expected, offset),
        window(actual, offset),
    )
}

/// The fixture tree must be complete before a byte comparison means anything:
/// 8 sources and 8 artifacts per harness, with no strays.
#[test]
fn fixtures_are_complete() {
    let sources = sources();
    assert_eq!(
        sources.len(),
        SOURCE_COUNT,
        "expected {SOURCE_COUNT} vendored source rules, found {}",
        sources.len()
    );

    for harness in registry::all() {
        let dir = fixtures_root().join(&harness.id);
        let mut found: Vec<PathBuf> = Vec::new();
        collect_markdown(&dir, &mut found);
        assert_eq!(
            found.len(),
            SOURCE_COUNT,
            "{} must vendor {SOURCE_COUNT} artifacts, found {} under {}",
            harness.id,
            found.len(),
            dir.display()
        );
    }
}

/// The load-bearing test: 8 sources × 4 harnesses = 32 byte-exact comparisons.
#[test]
fn renders_all_fixtures_byte_identically() {
    let mut comparisons = 0usize;
    let mut table = String::new();

    for (file_name, bytes) in sources() {
        let relative_path = format!("sources/{file_name}");
        let rule = parse_rule(&relative_path, &bytes)
            .unwrap_or_else(|e| panic!("{relative_path} must parse: {e:?}"));

        for harness in registry::all() {
            let rendered = render_rule_files(&rule, &harness.id)
                .unwrap_or_else(|e| panic!("{relative_path} → {} must render: {e:?}", harness.id));
            let actual = rendered
                .first()
                .unwrap_or_else(|| panic!("{} rendered no file", harness.id))
                .bytes
                .as_bytes();

            let expected_path = fixture_path(&harness.integration_path, &harness.id, &rule.slug);
            let expected = fs::read(&expected_path)
                .unwrap_or_else(|e| panic!("read {}: {e}", expected_path.display()));

            if let Some(offset) = first_divergence(&expected, actual) {
                panic!(
                    "{}",
                    divergence_report(
                        &format!(
                            "{} → {} (expected fixture {})",
                            relative_path,
                            harness.id,
                            expected_path.display()
                        ),
                        &expected,
                        actual,
                        offset,
                    )
                );
            }

            comparisons += 1;
            table.push_str(&format!(
                "  {:<12} {:<24} OK  {:>5} bytes\n",
                harness.id,
                rule.slug,
                actual.len()
            ));
        }
    }

    println!("{table}");
    println!("{comparisons}/{COMPARISON_COUNT} comparisons byte-identical");
    assert_eq!(
        comparisons, COMPARISON_COUNT,
        "every source must be compared against every harness"
    );
}

/// Collects `.md` files under `dir`, recursively, sorted.
fn collect_markdown(dir: &Path, out: &mut Vec<PathBuf>) {
    let entries = match fs::read_dir(dir) {
        Ok(entries) => entries,
        Err(_) => return,
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            collect_markdown(&path, out);
        } else if path.extension().and_then(|extension| extension.to_str()) == Some("md") {
            out.push(path);
        }
    }
    out.sort();
}

/// Rule files under `<checkout>/rules`, skipping any `_`-prefixed path segment
/// (`_schema.json` is not a rule) and non-`.md` files.
fn checkout_rule_files(rules_root: &Path) -> Vec<PathBuf> {
    let mut all = Vec::new();
    collect_markdown(rules_root, &mut all);
    all.into_iter()
        .filter(|path| {
            !path
                .strip_prefix(rules_root)
                .unwrap_or(path)
                .components()
                .any(|component| component.as_os_str().to_string_lossy().starts_with('_'))
        })
        .collect()
}

/// Staleness guard (ruling A14, `docs/BUILD.md`): re-render the live checkout's
/// `rules/` and diff its `integrations/`.
///
/// Defaults to the pinned `vendor/agency-rules` submodule, so `mise run parity`
/// works on a clean clone; `AGENCY_RULES_PARITY_ROOT` overrides it for an
/// unrelated checkout:
///
/// ```sh
/// AGENCY_RULES_PARITY_ROOT=/home/deuriib/projects/agency-rules \
///   cargo test --test rules_conformance -- --ignored rules_parity_against_checkout
/// ```
///
/// A source rule with **no** compiled artifact upstream is a reported finding,
/// not a failure: `agency-rules` gitignores its generated tree, so a fresh
/// checkout legitimately has no artifacts (and the checked-in one is known
/// stale — ruling C5).
#[test]
#[ignore = "needs an agency-rules checkout (defaults to vendor/agency-rules)"]
fn rules_parity_against_checkout() {
    let root = std::env::var_os("AGENCY_RULES_PARITY_ROOT")
        .map(PathBuf::from)
        .unwrap_or_else(|| {
            PathBuf::from(env!("CARGO_MANIFEST_DIR"))
                .parent()
                .expect("src-tauri has a parent")
                .join("vendor/agency-rules")
        });
    let rules_root = root.join("rules");
    assert!(
        rules_root.is_dir(),
        "{} has no rules/ directory — is the submodule initialised?",
        root.display()
    );
    let integrations = root.join("integrations");

    let mut compared = 0usize;
    let mut missing: Vec<String> = Vec::new();
    let mut failures: Vec<String> = Vec::new();

    for path in checkout_rule_files(&rules_root) {
        let relative_path = path
            .strip_prefix(&root)
            .unwrap_or(&path)
            .to_string_lossy()
            .replace('\\', "/");
        let bytes = fs::read(&path).unwrap_or_else(|e| panic!("read {}: {e}", path.display()));
        let rule = match parse_rule(&relative_path, &bytes) {
            Ok(rule) => rule,
            Err(error) => {
                failures.push(format!("{relative_path}: parse failed: {error:?}"));
                continue;
            }
        };

        for harness in registry::all() {
            let relative = harness.integration_path.replace("{{slug}}", &rule.slug);
            let artifact = integrations.join(&harness.id).join(&relative);
            let Ok(expected) = fs::read(&artifact) else {
                missing.push(format!(
                    "{} → {}: no compiled artifact at {}",
                    rule.slug,
                    harness.id,
                    artifact.display()
                ));
                continue;
            };

            let rendered = match render_rule_files(&rule, &harness.id) {
                Ok(files) => files,
                Err(error) => {
                    failures.push(format!(
                        "{} → {}: render failed: {error:?}",
                        rule.slug, harness.id
                    ));
                    continue;
                }
            };
            let actual = rendered
                .first()
                .map(|file| file.bytes.as_bytes())
                .unwrap_or_default();

            if let Some(offset) = first_divergence(&expected, actual) {
                failures.push(divergence_report(
                    &format!(
                        "{} → {} (upstream {})",
                        rule.slug,
                        harness.id,
                        artifact.display()
                    ),
                    &expected,
                    actual,
                    offset,
                ));
            } else {
                compared += 1;
            }
        }
    }

    for finding in &missing {
        eprintln!("REPORTED (not a failure): {finding}");
    }
    println!(
        "{compared} upstream artifact(s) byte-identical; {} reported without an artifact; {} divergence(s)",
        missing.len(),
        failures.len()
    );

    assert!(
        failures.is_empty(),
        "renderer/upstream divergence:\n\n{}",
        failures.join("\n\n")
    );
}
