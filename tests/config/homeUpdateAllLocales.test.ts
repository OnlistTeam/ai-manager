import { describe, expect, it } from "vitest";
import en from "@/i18n/locales/en.json";
import ja from "@/i18n/locales/ja.json";
import zhTW from "@/i18n/locales/zh-TW.json";
import zh from "@/i18n/locales/zh.json";

type TranslationTree = Record<string, unknown>;

function flattenStrings(
  value: unknown,
  path: string[] = [],
  result = new Map<string, string>(),
): Map<string, string> {
  if (typeof value === "string") {
    result.set(path.join("."), value);
  } else if (typeof value === "object" && value !== null) {
    for (const [key, child] of Object.entries(value)) {
      flattenStrings(child, [...path, key], result);
    }
  }
  return result;
}

function interpolationVariables(value: string): string[] {
  return Array.from(
    value.matchAll(/\{\{\s*([^}]+?)\s*\}\}/g),
    ([, name]) => name,
  ).sort();
}

const reference = flattenStrings(en.home.updateAll);
const locales = [
  ["zh", zh.home.updateAll],
  ["zh-TW", zhTW.home.updateAll],
  ["ja", ja.home.updateAll],
] as const;

describe("Home Update All locale coverage", () => {
  it.each(locales)("covers every key and variable in %s", (_name, tree) => {
    const actual = flattenStrings(tree as TranslationTree);
    expect([...actual.keys()].sort()).toEqual([...reference.keys()].sort());

    for (const [key, expected] of reference) {
      expect(interpolationVariables(actual.get(key) ?? "")).toEqual(
        interpolationVariables(expected),
      );
    }
  });
});
