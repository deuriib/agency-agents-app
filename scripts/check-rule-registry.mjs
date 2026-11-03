#!/usr/bin/env node
// Keep the rule-harness contract single-sourced on both sides.
//
// The rule harnesses (`antigravity`, `pi-agent`, `oh-my-pi`, `opencode`), their
// formats and their destinations are data, not code:
//
//     src-tauri/data/rules-tools.json   ← Rust `include_str!` + TS JSON import
//     src-tauri/data/rule-formats.json  ← Rust `include_str!` + TS JSON import
//
// A harness is installable iff its `format` appears in `rule-formats.json`, and
// both sides derive that the same way. They drifted once for agents — the TS
// list offered tools the backend refused (issues #49/#53) — so this guard fails
// the build when either side restates the list instead of reading the files.
//
// `singleSourceWiring` resolves the paths each source file declares and compares
// them to the shared files, so a typo'd relative path fails too.

import { readFileSync } from "node:fs";
import { dirname, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";

/** The two shared JSON files. Relative to the repository root. */
export const CONTRACT_PATH = "src-tauri/data/rules-tools.json";
export const FORMATS_PATH = "src-tauri/data/rule-formats.json";

const RUST_READER = "src-tauri/src/rules/registry.rs";
const TS_MIRROR = "src/lib/data/ruleRegistry.ts";

/** Validated rule-format list. */
export function parseRuleFormats(spec) {
  if (!Array.isArray(spec?.formats) || spec.formats.length === 0) {
    throw new Error("data/rule-formats.json: `formats` must be a non-empty array");
  }
  const formats = new Set(spec.formats);
  if (formats.size !== spec.formats.length) {
    throw new Error("data/rule-formats.json: duplicate format entries");
  }
  return formats;
}

/** The JSON files each reader actually declares, resolved to absolute paths. */
export function declaredSources(root) {
  const declared = (sourceRelPath, regex) => {
    // Production code only: a test module may embed the same literal to assert
    // on it, which would otherwise look like a second source.
    const text = productionSource(readFileSync(join(root, sourceRelPath), "utf8"));
    const base = dirname(join(root, sourceRelPath));
    const paths = [...text.matchAll(regex)].map((match) => resolve(base, match[1]));
    return [...new Set(paths)].sort();
  };
  return {
    rust: declared(RUST_READER, /include_str!\("([^"]+)"\)/g),
    ts: declared(TS_MIRROR, /from\s+"([^"]+\.json)"/g),
  };
}

/** Both sides must read the shared files rather than restate the list. */
export function singleSourceWiring(root) {
  const expected = [CONTRACT_PATH, FORMATS_PATH].map((path) => resolve(root, path));
  const { rust, ts } = declaredSources(root);
  return {
    rust: expected.every((path) => rust.includes(path)),
    frontend: expected.every((path) => ts.includes(path)),
  };
}

/** Harness ids the contract declares, sorted. */
export function harnessIds(contract) {
  return Object.keys(contract?.harnesses ?? {}).sort();
}

/** Harnesses this app can install: format appears in `rule-formats.json`. */
export function installableHarnesses(contract, formats) {
  return harnessIds(contract).filter((id) => formats.has(contract.harnesses[id].format));
}

/** Declared-but-unimplemented and implemented-but-undeclared formats. */
export function formatCoverage(contract, formats) {
  const declared = new Set(
    Object.values(contract?.harnesses ?? {}).map((harness) => harness.format),
  );
  return {
    declaredNotImplemented: [...declared].filter((format) => !formats.has(format)).sort(),
    implementedNotDeclared: [...formats].filter((format) => !declared.has(format)).sort(),
  };
}

/** Source with comments and (for Rust) the `#[cfg(test)]` module removed. */
export function productionSource(text) {
  return text
    .split("#[cfg(test)]")[0]
    .split("\n")
    .filter((line) => {
      const trimmed = line.trimStart();
      return !(
        trimmed.startsWith("//") ||
        trimmed.startsWith("/*") ||
        trimmed.startsWith("*")
      );
    })
    .join("\n");
}

/** Harness ids literally present in production source — a restated list. */
export function restatedHarnessIds(sourceText, ids) {
  const code = productionSource(sourceText);
  return ids.filter((id) => code.includes(id));
}

/** Read everything the guard needs from a checkout. */
export function compare(root) {
  const contract = JSON.parse(readFileSync(join(root, CONTRACT_PATH), "utf8"));
  const formats = parseRuleFormats(JSON.parse(readFileSync(join(root, FORMATS_PATH), "utf8")));
  const ids = harnessIds(contract);

  return {
    wiring: singleSourceWiring(root),
    literals: {
      rust: restatedHarnessIds(readFileSync(join(root, RUST_READER), "utf8"), ids),
      frontend: restatedHarnessIds(readFileSync(join(root, TS_MIRROR), "utf8"), ids),
    },
    coverage: formatCoverage(contract, formats),
    installable: installableHarnesses(contract, formats),
    ids,
  };
}

const isMain = process.argv[1] && resolve(process.argv[1]) === fileURLToPath(import.meta.url);

if (isMain) {
  const root = process.argv[2]
    ? resolve(process.argv[2])
    : resolve(dirname(fileURLToPath(import.meta.url)), "..");

  let result;
  try {
    result = compare(root);
  } catch (error) {
    console.error(`check-rule-registry: ${error.message}`);
    process.exit(2);
  }

  const problems = [];
  if (!result.wiring.rust) {
    problems.push(`${RUST_READER} no longer embeds both data/rule-*.json files — a literal list is back`);
  }
  if (!result.wiring.frontend) {
    problems.push(`${TS_MIRROR} no longer imports both data/rule-*.json files — a literal list is back`);
  }
  if (result.literals.rust.length) {
    problems.push(`harness ids hardcoded in ${RUST_READER}: ${result.literals.rust.join(", ")}`);
  }
  if (result.literals.frontend.length) {
    problems.push(`harness ids hardcoded in ${TS_MIRROR}: ${result.literals.frontend.join(", ")}`);
  }
  if (result.coverage.declaredNotImplemented.length) {
    problems.push(
      `harness formats with no native renderer: ${result.coverage.declaredNotImplemented.join(", ")}`,
    );
  }
  if (result.coverage.implementedNotDeclared.length) {
    problems.push(
      `implemented formats no harness declares: ${result.coverage.implementedNotDeclared.join(", ")}`,
    );
  }

  if (problems.length) {
    console.error("check-rule-registry: the rule-harness contract is out of sync");
    for (const problem of problems) console.error(`  - ${problem}`);
    process.exit(1);
  }

  console.log(
    `check-rule-registry: ${result.installable.length}/${result.ids.length} harnesses installable, both sides read the same contract`,
  );
}
