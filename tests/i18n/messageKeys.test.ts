import fs from "node:fs";
import path from "node:path";
import { describe, expect, it } from "vitest";
import { APP_LANGUAGES, type AppLanguage } from "@/i18n";
import de from "@/i18n/locales/de.json";
import en from "@/i18n/locales/en.json";
import es from "@/i18n/locales/es.json";
import fr from "@/i18n/locales/fr.json";
import id from "@/i18n/locales/id.json";
import itIT from "@/i18n/locales/it.json";
import ja from "@/i18n/locales/ja.json";
import ko from "@/i18n/locales/ko.json";
import ptBR from "@/i18n/locales/pt-BR.json";
import ru from "@/i18n/locales/ru.json";
import vi from "@/i18n/locales/vi.json";
import zhTW from "@/i18n/locales/zh-TW.json";
import zh from "@/i18n/locales/zh.json";

/**
 * This product's own copy namespaces. When a future task adds a new page,
 * add its top-level namespace here and the four-locale key-set equality
 * guard automatically covers it. The 60 existing upstream namespaces are not
 * on this list: their translation gaps are historical debt, and this test
 * should not turn them all red at once.
 */
const PRODUCT_NAMESPACES = [
  "error",
  "operation",
  "nav",
  "tool",
  "tools",
  "home",
  "taskCenter",
  "services",
  "extensions",
  "routing",
  "usage",
  "sessions",
  "preferences",
  "about",
  "data",
  "deeplink",
] as const;

/** Keys produced by src/native itself — invisible to the Rust registry, but still visible to the user. */
const NATIVE_MESSAGE_KEYS = [
  "error.native.responseSchemaMismatch",
  "error.native.unrecognized",
] as const;

const MESSAGE_KEYS_RS = path.resolve(
  __dirname,
  "..",
  "..",
  "src-tauri",
  "src",
  "domain",
  "message_keys.rs",
);

/** Pulls string literals out of the Rust registry, avoiding manual copies on both sides. */
function readRustRegistry(): string[] {
  const source = fs.readFileSync(MESSAGE_KEYS_RS, "utf8");
  const start = source.indexOf("USER_FACING_MESSAGE_KEYS: &[&str] = &[");
  expect(start).toBeGreaterThanOrEqual(0);
  const end = source.indexOf("];", start);
  expect(end).toBeGreaterThan(start);
  return Array.from(
    source.slice(start, end).matchAll(/"([^"]+)"/g),
    ([, key]) => key,
  ).filter((key) => key.includes("."));
}

function flatten(
  value: unknown,
  prefix = "",
  out = new Map<string, string>(),
): Map<string, string> {
  if (typeof value === "string") {
    out.set(prefix, value);
  } else if (typeof value === "object" && value !== null) {
    for (const [key, child] of Object.entries(value)) {
      flatten(child, prefix ? `${prefix}.${key}` : key, out);
    }
  }
  return out;
}

const LOCALES = [
  ["en", en],
  ["zh", zh],
  ["ja", ja],
  ["zh-TW", zhTW],
  ["ko", ko],
  ["de", de],
  ["fr", fr],
  ["es", es],
  ["pt-BR", ptBR],
  ["it", itIT],
  ["ru", ru],
  ["vi", vi],
  ["id", id],
] as const satisfies ReadonlyArray<readonly [AppLanguage, unknown]>;

const PLURAL_CATEGORIES = ["zero", "one", "two", "few", "many", "other"];
const PLURAL_SUFFIX = new RegExp(`_(${PLURAL_CATEGORIES.join("|")})$`);

/**
 * `a_one` and `a_few` are the same logical string in two grammars, so key-set
 * comparisons across locales work on the name without the category.
 */
function logicalKey(key: string): string {
  return key.replace(PLURAL_SUFFIX, "");
}

function isProductKey(key: string): boolean {
  return PRODUCT_NAMESPACES.some((ns) => key.startsWith(`${ns}.`));
}

/**
 * The plural categories a locale can actually select for a count this product
 * can show. French and Spanish have a `many` category, but it only applies from
 * a million upward; demanding a translation for a case no screen can reach
 * would be busywork, while Russian's `few` and `many` start at 2 and 5 and are
 * therefore required.
 */
function reachablePluralCategories(locale: string): Set<string> {
  const rules = new Intl.PluralRules(locale);
  const reachable = new Set<string>();
  for (let count = 0; count <= 2000; count += 1) {
    reachable.add(rules.select(count));
  }
  return reachable;
}

/** i18next pluralization: any plural category counts as covering the base key. */
function resolvable(flat: Map<string, string>, key: string): boolean {
  if (flat.has(key)) return true;
  return PLURAL_CATEGORIES.some((category) => flat.has(`${key}_${category}`));
}

function productKeys(tree: unknown): string[] {
  return [
    ...new Set(
      [...flatten(tree).keys()]
        .filter((key) =>
          PRODUCT_NAMESPACES.some(
            (ns) => key === ns || key.startsWith(`${ns}.`),
          ),
        )
        .map(logicalKey),
    ),
  ].sort();
}

describe("message key coverage", () => {
  it.each(LOCALES)(
    "translates every backend message_key in %s",
    (_name, tree) => {
      const flat = flatten(tree);
      const missing = [...readRustRegistry(), ...NATIVE_MESSAGE_KEYS].filter(
        (key) => !resolvable(flat, key),
      );
      expect(missing).toEqual([]);
    },
  );

  it("keeps the product namespaces identical across every locale", () => {
    const reference = productKeys(en);
    expect(reference.length).toBeGreaterThan(0);
    for (const [name, tree] of LOCALES) {
      expect(productKeys(tree), `${name} diverged`).toEqual(reference);
    }
  });

  it("ships a locale file for every language the picker offers", () => {
    expect(LOCALES.map(([name]) => name).sort()).toEqual(
      [...APP_LANGUAGES].sort(),
    );
  });

  /**
   * A count rendered with the wrong plural form is the failure mode nobody
   * notices in review and every native reader notices immediately: Russian
   * needs four forms for ordinary numbers, Korean one, German two. Asserting
   * against `Intl.PluralRules` means the expectation comes from CLDR rather
   * than from whatever the translator assumed.
   */
  it.each(LOCALES)(
    "carries the plural forms %s actually uses",
    (name, tree) => {
      const expected = reachablePluralCategories(name);
      const flat = flatten(tree);
      const groups = new Set(
        [...flatten(en).keys()]
          .filter((key) => PLURAL_SUFFIX.test(key) && isProductKey(key))
          .map(logicalKey),
      );
      expect(groups.size).toBeGreaterThan(0);

      const wrong: string[] = [];
      for (const group of groups) {
        for (const category of expected) {
          if (!flat.has(`${group}_${category}`)) {
            wrong.push(`${group}_${category} missing`);
          }
        }
      }
      expect(wrong, `${name} has wrong plural coverage`).toEqual([]);
    },
  );

  /**
   * A dropped `{{name}}` renders as a sentence with a hole in it, and a renamed
   * one renders the placeholder verbatim. Neither shows up until someone reads
   * that screen in that language.
   *
   * Plural siblings are the one place a translation may legitimately differ:
   * English writes "the latest one" without a count where Chinese writes
   * "最近 1 份" with one. So a sibling only has to stay inside the placeholders
   * the English group uses, while `_other`, which every language selects, has
   * to match exactly.
   */
  it.each(LOCALES)("preserves every interpolation in %s", (name, tree) => {
    const tokens = (text: string) =>
      [...text.matchAll(/\{\{\s*([^}]+?)\s*\}\}/g)].map(([, token]) => token);

    const english = flatten(en);
    const groupTokens = new Map<string, Set<string>>();
    for (const [key, value] of english) {
      const group = logicalKey(key);
      const seen = groupTokens.get(group) ?? new Set<string>();
      for (const token of tokens(value)) seen.add(token);
      groupTokens.set(group, seen);
    }

    const broken: string[] = [];
    for (const [key, value] of flatten(tree)) {
      if (!isProductKey(key)) continue;
      const reference = english.get(key);
      if (reference === undefined) continue;

      const isSibling = PLURAL_SUFFIX.test(key) && !key.endsWith("_other");
      if (isSibling) {
        const allowed = groupTokens.get(logicalKey(key)) ?? new Set<string>();
        const extra = tokens(value).filter((token) => !allowed.has(token));
        if (extra.length > 0) broken.push(`${key}: unknown {{${extra}}}`);
        continue;
      }

      const want = [...tokens(reference)].sort().join(",");
      const got = [...tokens(value)].sort().join(",");
      if (want !== got)
        broken.push(`${key}: expected {{${want}}} got {{${got}}}`);
    }
    expect(broken, `${name} lost interpolations`).toEqual([]);
  });

  it.each(LOCALES)("leaves no blank product copy in %s", (_name, tree) => {
    const blank = [...flatten(tree)]
      .filter(([key]) =>
        PRODUCT_NAMESPACES.some((ns) => key.startsWith(`${ns}.`)),
      )
      .filter(([, value]) => value.trim().length === 0)
      .map(([key]) => key);
    expect(blank).toEqual([]);
  });

  it("never leaks technical vocabulary into user-facing copy", () => {
    const flat = flatten(en);
    // These labels live inside an explicit, collapsed process-log disclosure.
    // They are technical by design and never appear in normal product copy.
    const technicalDisclosureKeys = new Set([
      "taskCenter.logs.kind.stdout",
      "taskCenter.logs.kind.stderr",
    ]);
    const banned = [
      "stderr",
      "stdout",
      "exit code",
      "ENOENT",
      "panicked",
      "npm ",
      "shell",
      "Traceback",
    ];
    const offenders = [...flat]
      .filter(([key]) =>
        PRODUCT_NAMESPACES.some((ns) => key.startsWith(`${ns}.`)),
      )
      .filter(([key]) => !technicalDisclosureKeys.has(key))
      .filter(([, value]) =>
        banned.some((word) => value.toLowerCase().includes(word.toLowerCase())),
      )
      .map(([key]) => key);
    expect(offenders).toEqual([]);
  });

  it("describes every tool the backend can report", async () => {
    const { toolIdSchema } = await import("@/native");
    for (const [name, tree] of LOCALES) {
      const flat = flatten(tree);
      const missing = toolIdSchema.options.filter(
        (id: string) => !flat.has(`tool.${id}.description`),
      );
      expect(missing, `${name} is missing tool descriptions`).toEqual([]);
    }
  });

  it("describes every desktop app the backend can report", async () => {
    const { desktopAppIdSchema } = await import("@/native");
    for (const [name, tree] of LOCALES) {
      const flat = flatten(tree);
      const missing = desktopAppIdSchema.options.flatMap((id: string) =>
        ["surface", "description"]
          .map((field) => `tools.desktopApps.items.${id}.${field}`)
          .filter((key) => !flat.has(key)),
      );
      expect(missing, `${name} is missing desktop app copy`).toEqual([]);
    }
  });

  it.each(LOCALES)("describes the import source in %s", (name, tree) => {
    const importCopy = [...flatten(tree)]
      .filter(([key]) => key.startsWith("preferences.import."))
      .map(([, value]) => value)
      .join("\n");

    expect(importCopy.length, `${name} import copy is missing`).toBeGreaterThan(
      0,
    );
  });
});
