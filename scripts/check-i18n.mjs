#!/usr/bin/env node
// Keep the Rules workspace translated in every locale.
//
// `en.ts` defines `MessageKey`, and every other locale is a
// `satisfies Partial<Messages>` override that silently falls back to English
// for anything it omits. That fallback is a safety net, not a translation
// strategy: a key missing from `de.ts` ships an English hole to German users,
// and `svelte-check` cannot see it because the locale is partial on purpose.
//
// This guard covers only the namespaces this milestone introduced
// (`rules.*`, `nav.rules`, `palette.openRules`). The locale files still carry
// pre-existing gaps from earlier work; widening the guard would fail for
// reasons unrelated to the rules PR (ruling A12 / TASK 23).
//
// Placeholders are compared too: a translated `{count}` that became `{anzahl}`
// type-checks and renders a literal "{anzahl}" at runtime.

import { readFileSync, readdirSync } from "node:fs";
import { dirname, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";

/** Directory holding `<locale>.ts`, relative to the repository root. */
export const LOCALE_DIR = "src/lib/i18n/locales";
/** The locale that defines the key set. */
export const BASE_LOCALE = "en";

/** Prefixes (and one exact key) this guard is responsible for. */
const GUARDED_PREFIXES = ["rules."];
const GUARDED_KEYS = new Set(["nav.rules", "palette.openRules"]);

/** A single `"key": "value",` line of a locale object literal. */
const ENTRY_PATTERN = /^\s*"((?:[^"\\]|\\.)+)"\s*:\s*"((?:[^"\\]|\\.)*)"\s*,?\s*$/;
const PLACEHOLDER_PATTERN = /\{([^{}]+)\}/g;

/** True when this guard owns the key. */
export function isGuarded(key) {
  return GUARDED_KEYS.has(key) || GUARDED_PREFIXES.some((prefix) => key.startsWith(prefix));
}

/**
 * Key → raw value for every single-line entry in a locale file.
 *
 * A regex is enough because the locale objects are flat, one entry per line:
 * the `Messages` type is a flat dotted-key record and `npm run check` enforces
 * it. Multi-line or nested values would not type-check, so they cannot appear.
 */
export function extractEntries(sourceText) {
  const entries = new Map();
  for (const line of sourceText.split("\n")) {
    const match = ENTRY_PATTERN.exec(line);
    if (match === null) continue;
    entries.set(match[1], match[2]);
  }
  return entries;
}

/** Every key declared in a locale file, in file order. */
export function extractKeys(sourceText) {
  return [...extractEntries(sourceText).keys()];
}

/** The guarded subset of `keys`, in the order given. */
export function guardedKeys(keys) {
  return keys.filter(isGuarded);
}

/** Base keys the locale does not declare. */
export function missingKeys(baseKeys, localeKeys) {
  const present = new Set(localeKeys);
  return baseKeys.filter((key) => !present.has(key));
}

/** Locale keys the base locale does not declare — a typo or a stray key. */
export function extraKeys(baseKeys, localeKeys) {
  const known = new Set(baseKeys);
  return localeKeys.filter((key) => !known.has(key));
}

/** Placeholder names inside a value, sorted so comparison is order-independent. */
export function placeholders(value) {
  return [...new Set([...value.matchAll(PLACEHOLDER_PATTERN)].map((match) => match[1]))].sort();
}

/** Keys whose translated value carries a different placeholder set than `en`. */
export function placeholderMismatches(baseEntries, localeEntries) {
  const mismatches = [];
  for (const [key, value] of baseEntries) {
    if (!localeEntries.has(key)) continue;
    const expected = placeholders(value);
    const actual = placeholders(localeEntries.get(key));
    if (expected.join(",") !== actual.join(",")) {
      mismatches.push({ key, expected, actual });
    }
  }
  return mismatches;
}

/** Locale ids (file names without `.ts`) present on disk, sorted, base first. */
export function localeIds(root) {
  const ids = readdirSync(join(root, LOCALE_DIR))
    .filter((file) => file.endsWith(".ts"))
    .map((file) => file.slice(0, -".ts".length))
    .sort();
  return [BASE_LOCALE, ...ids.filter((id) => id !== BASE_LOCALE)];
}

/** Compare the guarded namespace of every locale against `en.ts`. */
export function compare(root) {
  const read = (locale) => readFileSync(join(root, LOCALE_DIR, `${locale}.ts`), "utf8");
  const baseAll = extractEntries(read(BASE_LOCALE));
  const baseKeys = guardedKeys([...baseAll.keys()]);
  const baseEntries = new Map(baseKeys.map((key) => [key, baseAll.get(key)]));

  const locales = localeIds(root)
    .filter((locale) => locale !== BASE_LOCALE)
    .map((locale) => {
      const entries = extractEntries(read(locale));
      const keys = [...entries.keys()];
      return {
        locale,
        keys: keys.length,
        guarded: guardedKeys(keys).length,
        missing: missingKeys(baseKeys, keys),
        extra: extraKeys(baseKeys, guardedKeys(keys)),
        mismatches: placeholderMismatches(baseEntries, entries),
      };
    });

  return { guarded: baseKeys.length, locales };
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
    console.error(`check-i18n: ${error.message}`);
    process.exit(2);
  }

  const problems = [];
  for (const { locale, missing, extra, mismatches } of result.locales) {
    if (missing.length) {
      problems.push(`${locale}: missing ${missing.length} guarded key(s): ${missing.join(", ")}`);
    }
    if (extra.length) {
      problems.push(`${locale}: ${extra.length} guarded key(s) not in ${BASE_LOCALE}.ts: ${extra.join(", ")}`);
    }
    for (const { key, expected, actual } of mismatches) {
      problems.push(
        `${locale}: ${key} has placeholders {${actual.join("}, {")}} but ${BASE_LOCALE} has {${expected.join("}, {")}}`,
      );
    }
  }

  if (problems.length) {
    console.error("check-i18n: the rules namespace is not translated in every locale");
    for (const problem of problems) console.error(`  - ${problem}`);
    process.exit(1);
  }

  const clean = result.locales.filter((locale) => locale.missing.length === 0).length;
  console.log(
    `check-i18n: ${result.guarded} guarded keys present in ${clean}/${result.locales.length} locales, placeholders preserved`,
  );
}
