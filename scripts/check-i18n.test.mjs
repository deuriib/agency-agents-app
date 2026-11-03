import { test } from "node:test";
import assert from "node:assert/strict";
import { execFileSync } from "node:child_process";
import { cpSync, mkdtempSync, readFileSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { dirname, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";
import {
  BASE_LOCALE,
  LOCALE_DIR,
  compare,
  extractEntries,
  extractKeys,
  extraKeys,
  guardedKeys,
  isGuarded,
  localeIds,
  missingKeys,
  placeholderMismatches,
  placeholders,
} from "./check-i18n.mjs";

const HERE = dirname(fileURLToPath(import.meta.url));
const ROOT = resolve(HERE, "..");
const SCRIPT = join(HERE, "check-i18n.mjs");

function readLocale(locale) {
  return readFileSync(join(ROOT, LOCALE_DIR, `${locale}.ts`), "utf8");
}

/** A throwaway copy of the locale directory, safe to mutate. */
function fixture() {
  const dir = mkdtempSync(join(tmpdir(), "i18n-"));
  cpSync(join(ROOT, LOCALE_DIR), join(dir, LOCALE_DIR), { recursive: true });
  return dir;
}

/** Run the checker against `root` and require a clean failure. */
function expectFailure(root, pattern) {
  assert.throws(
    () => execFileSync(process.execPath, [SCRIPT, root], { stdio: "pipe" }),
    (error) => {
      assert.equal(error.status, 1);
      assert.match(String(error.stderr), pattern);
      return true;
    },
  );
}

test("the repository locale files pass the guard", () => {
  const out = execFileSync(process.execPath, [SCRIPT], { cwd: ROOT, encoding: "utf8" });
  assert.match(out, /guarded keys present in 11\/11 locales, placeholders preserved/);
});

test("the guard covers exactly the namespaces this milestone added", () => {
  assert.equal(isGuarded("rules.subtitle"), true);
  assert.equal(isGuarded("nav.rules"), true);
  assert.equal(isGuarded("palette.openRules"), true);
  assert.equal(isGuarded("rulesX"), false);
  assert.equal(isGuarded("nav.runbooks"), false);
  assert.equal(isGuarded("palette.openRunbooks"), false);
});

test("extractEntries reads only flat quoted entries", () => {
  const source = [
    'import type { Messages } from "./en";',
    "",
    "const xx = {",
    '  "rules.count": "{count} rules",',
    '  "rules.list.itemLabel": "{name} · {category}",',
    "  // a commented-out key is not a key",
    '  "about.quoted": "she said \\"hi\\"",',
    "} satisfies Partial<Messages>;",
  ].join("\n");

  const entries = extractEntries(source);
  assert.deepEqual([...entries.keys()], ["rules.count", "rules.list.itemLabel", "about.quoted"]);
  assert.equal(entries.get("about.quoted"), 'she said \\"hi\\"');
  assert.deepEqual(extractKeys(source), [...entries.keys()]);
});

test("missingKeys is order-stable and reports only absent keys", () => {
  assert.deepEqual(missingKeys(["a", "b", "c"], ["c", "a"]), ["b"]);
  assert.deepEqual(missingKeys(["a"], ["a"]), []);
  assert.deepEqual(extraKeys(["a"], ["a", "rules.typo"]), ["rules.typo"]);
});

test("placeholders are compared as a set, not a sequence", () => {
  assert.deepEqual(placeholders("{rule} for {harness} at {dest}"), ["dest", "harness", "rule"]);
  assert.deepEqual(placeholders("no placeholders"), []);
  assert.deepEqual(placeholders("{a} and {a} again"), ["a"]);

  const base = new Map([["k", "{count} rules"]]);
  assert.deepEqual(placeholderMismatches(base, new Map([["k", "{anzahl} Regeln"]])), [
    { key: "k", expected: ["count"], actual: ["anzahl"] },
  ]);
  assert.deepEqual(placeholderMismatches(base, new Map([["k", "{count} Regeln"]])), []);
  assert.deepEqual(placeholderMismatches(base, new Map()), [], "a missing key is not a mismatch");
});

test("every guarded key exists in all eleven other locales with the same placeholders", () => {
  const { guarded, locales } = compare(ROOT);

  assert.ok(guarded > 0, "en.ts declares no guarded keys");
  assert.equal(locales.length, 11, "there are eleven other locales, not more or fewer");

  const baseEntries = new Map(
    guardedKeys([...extractEntries(readLocale(BASE_LOCALE)).keys()]).map((key) => [
      key,
      extractEntries(readLocale(BASE_LOCALE)).get(key),
    ]),
  );

  for (const locale of locales) {
    const entries = extractEntries(readLocale(locale.locale));
    const missing = missingKeys([...baseEntries.keys()], [...entries.keys()]);
    const extra = extraKeys([...baseEntries.keys()], guardedKeys([...entries.keys()]));
    const mismatches = placeholderMismatches(baseEntries, entries);

    assert.deepEqual(missing, [], `${locale.locale}.ts is missing guarded keys`);
    assert.deepEqual(extra, [], `${locale.locale}.ts has guarded keys en.ts does not declare`);
    assert.deepEqual(mismatches, [], `${locale.locale}.ts changed a placeholder name`);
    assert.equal(locale.guarded, guarded, `${locale.locale}.ts must translate every guarded key`);
  }

  console.log(
    `guarded keys: ${guarded}; per-locale totals: ${locales
      .map((locale) => `${locale.locale}=${locale.guarded}`)
      .join(", ")}`,
  );
});

test("localeIds lists the base locale first and every file exactly once", () => {
  const ids = localeIds(ROOT);
  assert.equal(ids[0], BASE_LOCALE);
  assert.equal(new Set(ids).size, ids.length);
  assert.equal(ids.length, 12);
});

test("a locale missing a guarded key fails the guard", () => {
  const dir = fixture();
  try {
    const file = join(dir, LOCALE_DIR, "de.ts");
    const source = readFileSync(file, "utf8");
    const mutated = source.replace(/^ {2}"rules\.count": .*\n/m, "");
    assert.notEqual(mutated, source, "fixture no longer contains rules.count");
    writeFileSync(file, mutated);

    expectFailure(dir, /de: missing 1 guarded key\(s\): rules\.count/);
  } finally {
    rmSync(dir, { recursive: true, force: true });
  }
});

test("a renamed placeholder fails the guard", () => {
  const dir = fixture();
  try {
    const file = join(dir, LOCALE_DIR, "ja.ts");
    const source = readFileSync(file, "utf8");
    const mutated = source.replace('"rules.count": "{count}', '"rules.count": "{件数}');
    assert.notEqual(mutated, source, "fixture no longer contains the rules.count placeholder");
    writeFileSync(file, mutated);

    expectFailure(dir, /ja: rules\.count has placeholders \{件数\} but en has \{count\}/);
  } finally {
    rmSync(dir, { recursive: true, force: true });
  }
});

test("a guarded key that en.ts does not declare fails the guard", () => {
  const dir = fixture();
  try {
    const file = join(dir, LOCALE_DIR, "fr.ts");
    const source = readFileSync(file, "utf8");
    writeFileSync(file, source.replace('"rules.retry"', '"rules.retryy"'));
    expectFailure(dir, /fr: 1 guarded key\(s\) not in en\.ts: rules\.retryy/);
  } finally {
    rmSync(dir, { recursive: true, force: true });
  }
});
