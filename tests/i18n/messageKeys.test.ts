import fs from "node:fs";
import path from "node:path";
import { describe, expect, it } from "vitest";
import en from "@/i18n/locales/en.json";
import ja from "@/i18n/locales/ja.json";
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
] as const;

/** i18next pluralization: `a_one` / `a_other` both count as covering `a`. */
function resolvable(flat: Map<string, string>, key: string): boolean {
  return flat.has(key) || flat.has(`${key}_one`) || flat.has(`${key}_other`);
}

function productKeys(tree: unknown): string[] {
  return [...flatten(tree).keys()]
    .filter((key) =>
      PRODUCT_NAMESPACES.some((ns) => key === ns || key.startsWith(`${ns}.`)),
    )
    .sort();
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
