import { test } from "node:test";
import assert from "node:assert/strict";
import { execFileSync } from "node:child_process";
import {
  cpSync,
  mkdirSync,
  mkdtempSync,
  readFileSync,
  rmSync,
  writeFileSync,
} from "node:fs";
import { tmpdir } from "node:os";
import { dirname, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";
import {
  CONTRACT_PATH,
  FORMATS_PATH,
  compare,
  declaredSources,
  formatCoverage,
  harnessIds,
  installableHarnesses,
  parseRuleFormats,
  restatedHarnessIds,
  singleSourceWiring,
} from "./check-rule-registry.mjs";

const HERE = dirname(fileURLToPath(import.meta.url));
const ROOT = resolve(HERE, "..");
const SCRIPT = join(HERE, "check-rule-registry.mjs");
const RUST_READER = "src-tauri/src/rules/registry.rs";
const TS_MIRROR = "src/lib/data/ruleRegistry.ts";

function readJson(relativePath) {
  return JSON.parse(readFileSync(join(ROOT, relativePath), "utf8"));
}

/** A throwaway copy of every input the guard reads, safe to mutate. */
function fixture() {
  const dir = mkdtempSync(join(tmpdir(), "rule-registry-"));
  for (const path of [
    CONTRACT_PATH,
    FORMATS_PATH,
    RUST_READER,
    TS_MIRROR,
  ]) {
    mkdirSync(dirname(join(dir, path)), { recursive: true });
    cpSync(join(ROOT, path), join(dir, path));
  }
  return dir;
}

/** Run the checker against `dir` and require a clean failure. */
function expectFailure(dir, pattern) {
  assert.throws(
    () => execFileSync(process.execPath, [SCRIPT, dir], { stdio: "pipe" }),
    (error) => {
      assert.equal(error.status, 1);
      assert.match(String(error.stderr), pattern);
      return true;
    },
  );
}

test("the repository rule contract passes the guard", () => {
  const out = execFileSync(process.execPath, [SCRIPT], { cwd: ROOT, encoding: "utf8" });
  assert.match(out, /4\/4 harnesses installable, both sides read the same contract/);
});

test("both sides are wired to the shared contract files", () => {
  assert.deepEqual(singleSourceWiring(ROOT), { rust: true, frontend: true });

  const { rust, ts } = declaredSources(ROOT);
  const expected = [CONTRACT_PATH, FORMATS_PATH].map((path) => resolve(ROOT, path)).sort();
  assert.deepEqual(rust, expected, "registry.rs must embed both rule contracts");
  assert.deepEqual(ts, expected, "ruleRegistry.ts must import both rule contracts");
});

/**
 * The two-sides-cannot-drift proof. Each side's declared source is resolved and
 * parsed independently, so the arrays can only be equal if both point at the
 * same contract. The Rust reader's own test asserts the same set loads.
 */
test("rust and ts derive the same harness set", () => {
  const { rust, ts } = declaredSources(ROOT);
  const contractFor = (files) => files.find((file) => file.endsWith("rules-tools.json"));
  const formatsFor = (files) => files.find((file) => file.endsWith("rule-formats.json"));

  const rustIds = harnessIds(JSON.parse(readFileSync(contractFor(rust), "utf8")));
  const tsIds = harnessIds(JSON.parse(readFileSync(contractFor(ts), "utf8")));
  assert.deepEqual(rustIds, tsIds, "both readers must load the same harness ids");

  const rustFormats = parseRuleFormats(JSON.parse(readFileSync(formatsFor(rust), "utf8")));
  const tsFormats = parseRuleFormats(JSON.parse(readFileSync(formatsFor(ts), "utf8")));
  assert.deepEqual(
    installableHarnesses(JSON.parse(readFileSync(contractFor(rust), "utf8")), rustFormats),
    installableHarnesses(JSON.parse(readFileSync(contractFor(ts), "utf8")), tsFormats),
    "both readers must derive the same installable set",
  );

  assert.deepEqual(rustIds, ["antigravity", "ohMyPi", "opencode", "piAgent"]);
});

test("format coverage has zero orphans on both sides", () => {
  const contract = readJson(CONTRACT_PATH);
  const formats = parseRuleFormats(readJson(FORMATS_PATH));
  assert.deepEqual(formatCoverage(contract, formats), {
    declaredNotImplemented: [],
    implementedNotDeclared: [],
  });
  assert.deepEqual(installableHarnesses(contract, formats), harnessIds(contract));
});

test("a reintroduced literal harness list fails the guard", () => {
  const dir = fixture();
  try {
    const file = join(dir, TS_MIRROR);
    const source = readFileSync(file, "utf8");
    const mutated = source.replace(
      "const IMPLEMENTED_FORMATS",
      'export const HARDCODED = ["antigravity", "piAgent"];\nconst IMPLEMENTED_FORMATS',
    );
    assert.notEqual(mutated, source, "fixture no longer contains the expected anchor");
    writeFileSync(file, mutated);

    const ids = harnessIds(readJson(CONTRACT_PATH));
    assert.deepEqual(restatedHarnessIds(mutated, ids), ["antigravity", "piAgent"]);
    expectFailure(dir, /harness ids hardcoded in src\/lib\/data\/ruleRegistry\.ts/);
  } finally {
    rmSync(dir, { recursive: true, force: true });
  }
});

test("a reader pointing at a different contract fails the guard", () => {
  const dir = fixture();
  try {
    // The TS side reads a divergent contract (one harness fewer) while the Rust
    // side keeps the shared one: the two would derive different sets.
    const contract = readJson(CONTRACT_PATH);
    delete contract.harnesses["oh-my-pi"];
    writeFileSync(join(dir, "src-tauri/data/rules-tools-other.json"), JSON.stringify(contract));

    const file = join(dir, TS_MIRROR);
    writeFileSync(
      file,
      readFileSync(file, "utf8").replace(
        'data/rules-tools.json"',
        'data/rules-tools-other.json"',
      ),
    );

    assert.deepEqual(singleSourceWiring(dir), { rust: true, frontend: false });
    expectFailure(dir, /no longer imports both data\/rule-\*\.json files/);
  } finally {
    rmSync(dir, { recursive: true, force: true });
  }
});

/**
 * The bundled `harnesses` object must stay a verbatim copy of the upstream
 * repository. CI has no sibling checkout, so this skips cleanly unless
 * `AGENCY_RULES_ROOT` points at one — the same pattern as the agency-rules
 * parity test.
 */
test(
  "bundled harnesses are verbatim upstream",
  { skip: !process.env.AGENCY_RULES_ROOT && "AGENCY_RULES_ROOT is not set" },
  () => {
    const upstream = JSON.parse(
      readFileSync(join(process.env.AGENCY_RULES_ROOT, "tools.json"), "utf8"),
    );
    assert.deepEqual(readJson(CONTRACT_PATH).harnesses, upstream.harnesses);
  },
);

test("compare reports the resolved wiring and installable set", () => {
  const result = compare(ROOT);
  assert.deepEqual(result.wiring, { rust: true, frontend: true });
  assert.deepEqual(result.literals, { rust: [], frontend: [] });
  assert.deepEqual(result.installable, result.ids);
});
