#!/usr/bin/env node
// Architecture boundary checker (ADR-0003 rule 3, AI_RULES rule 1/2/7).
// Zero dependencies on purpose: runs as `node scripts/check-boundaries.mjs`.

import { readdirSync, readFileSync, statSync } from "node:fs";
import { join, relative } from "node:path";
import { fileURLToPath } from "node:url";

const ROOT = fileURLToPath(new URL("..", import.meta.url));

// R1/R2 are an ALLOWLIST, not a denylist. A denylist of upstream module names
// can only ever catch the names someone remembered to list: Phase 1 shipped an
// infrastructure path module referencing upstream roots that the seven-name
// denylist did not know about. Every `crate::` root that is not a product layer
// is upstream by definition and must be reached through `crate::compat::ccswitch`.
const ALLOWED_CRATE_ROOTS = new Set([
  "domain",
  "platform",
  "adapters",
  "application",
  "repositories",
  "infrastructure",
  "compat",
]);

// The single documented exemption (ADR-0002): explicit CC Switch source
// discovery has to consult the compatibility config module directly. Product
// writes no longer use the inherited path override.
// Any new entry here requires an ADR.
const CRATE_ROOT_EXEMPTIONS = new Map([
  ["src-tauri/src/infrastructure/paths.rs", new Set(["config"])],
]);

// Matches `use tauri...` / `pub use tauri...` statements and inline `tauri::`
// path references. Whole-line `//` comments are skipped separately so prose
// mentioning "tauri" does not false-positive.
const TAURI_PATTERN = /\btauri::|\buse\s+tauri\b/g;
const TAURI_IMPORT_PATTERN =
  /(?:from\s*|import\s*\(\s*|require\s*\(\s*)["']@tauri-apps\//g;

// R1: pure layers. R2: product layers that may only reach upstream via the
// facade. R3: src-tauri/src/compat is the single exempt directory, absent here.
const R1_DIRS = ["src-tauri/src/domain", "src-tauri/src/platform"];
const R2_DIRS = [
  "src-tauri/src/application",
  "src-tauri/src/adapters",
  "src-tauri/src/repositories",
  "src-tauri/src/infrastructure",
];

// Layers not created yet; their absence is expected and must not fail the
// check. Every other configured directory (including the frontend `src/` root)
// is required to exist.
const OPTIONAL_DIRS = new Set(["src-tauri/src/repositories"]);

// R4: frontend Tauri access is limited to the native layer and the narrow
// bootstrap/infrastructure exceptions below.
const R4_ALLOWED_DIRS = ["src/native"];
const R4_ALLOWLIST = new Set([
  "src/main.tsx",
  "src/components/DatabaseUpgrade.tsx",
  "src/lib/frontendLogger.ts",
  "src/lib/windowActivity.ts",
]);

// R5: the app shell and the pages reach the backend through entities/features
// hooks. R4 stops a component from importing @tauri-apps; R5 stops the same
// component from importing the native client one layer down, which would put
// IPC calls back inside presentation code (spec section 20/80).
const R5_DIRS = ["src/app", "src/pages"];
const NATIVE_IMPORT_PATTERN =
  /(?:from\s*|import\s*\(\s*)["'](?:@\/native|(?:\.\.?\/)+native)(?:\/[^"']*)?["']/g;

const IDENT_START = /[A-Za-z_]/;
const IDENT_CHAR = /[A-Za-z0-9_]/;

const toPosix = (value) => value.split("\\").join("/");

function walk(dir, extensions, out = []) {
  for (const entry of readdirSync(dir)) {
    const full = join(dir, entry);
    if (statSync(full).isDirectory()) {
      walk(full, extensions, out);
    } else if (extensions.some((ext) => entry.endsWith(ext))) {
      out.push(full);
    }
  }
  return out;
}

// Resolves a configured directory relative to ROOT, walks it, and returns its
// files. A missing directory is a hard error naming the offending path, unless
// that path is explicitly allowed to not exist yet (OPTIONAL_DIRS).
function walkDir(relDir, extensions) {
  const full = join(ROOT, relDir);
  let stats;
  try {
    stats = statSync(full);
  } catch {
    if (OPTIONAL_DIRS.has(relDir)) return [];
    console.error(
      `Architecture boundary check misconfigured: required directory "${relDir}" does not exist.`,
    );
    process.exit(1);
  }
  if (!stats.isDirectory()) {
    console.error(
      `Architecture boundary check misconfigured: "${relDir}" exists but is not a directory.`,
    );
    process.exit(1);
  }
  return walk(full, extensions);
}

function readIdent(content, start) {
  if (start >= content.length || !IDENT_START.test(content[start])) return null;
  let end = start;
  while (end < content.length && IDENT_CHAR.test(content[end])) end += 1;
  return content.slice(start, end);
}

function pushHead(heads, content, memberStart) {
  let cursor = memberStart;
  while (cursor < content.length && /\s/.test(content[cursor])) cursor += 1;
  const ident = readIdent(content, cursor);
  if (ident) heads.push(ident);
}

// Reads the head identifiers introduced right after `crate::`. A `{` opens a
// group whose depth-1 commas separate members; nested groups are skipped by the
// depth counter, so `crate::{domain::{A, B}, config}` yields `domain, config`.
function readSegmentHeads(content, start) {
  let cursor = start;
  while (cursor < content.length && /\s/.test(content[cursor])) cursor += 1;
  if (content[cursor] !== "{") {
    const ident = readIdent(content, cursor);
    return ident ? [ident] : [];
  }
  const heads = [];
  let depth = 0;
  let memberStart = cursor + 1;
  for (let i = cursor; i < content.length; i += 1) {
    const char = content[i];
    if (char === "{") {
      depth += 1;
      if (depth === 1) memberStart = i + 1;
      continue;
    }
    if (char === "}") {
      depth -= 1;
      if (depth === 0) {
        pushHead(heads, content, memberStart);
        return heads;
      }
      continue;
    }
    if (char === "," && depth === 1) {
      pushHead(heads, content, memberStart);
      memberStart = i + 1;
    }
  }
  return heads;
}

// Finds every `crate::` reference and returns the first path segment of each
// item it introduces, together with the byte offset for line reporting.
function collectCrateRoots(content) {
  const found = [];
  const needle = "crate::";
  let index = content.indexOf(needle);
  while (index !== -1) {
    const after = index + needle.length;
    const before = index === 0 ? "" : content[index - 1];
    // `my_crate::` and `foo::crate::` are not the crate root keyword.
    if (IDENT_CHAR.test(before) || before === ":") {
      index = content.indexOf(needle, after);
      continue;
    }
    for (const name of readSegmentHeads(content, after)) {
      found.push({ name, index });
    }
    index = content.indexOf(needle, after);
  }
  return found;
}

const violations = [];
const usedExemptions = new Set();

function lineNumberAt(content, index) {
  return content.slice(0, index).split("\n").length;
}

// Runs every check's pattern against the whole file content (not line by line),
// so multi-line constructs like grouped `use crate::{...}` imports are caught.
// Each match is still mapped back to a line number for reporting.
function scan(file, relPath, checks) {
  const content = readFileSync(file, "utf8");
  const lines = content.split("\n");
  for (const check of checks) {
    const pattern = new RegExp(check.pattern.source, check.pattern.flags);
    let match;
    while ((match = pattern.exec(content)) !== null) {
      const lineNum = content.slice(0, match.index).split("\n").length;
      const lineText = lines[lineNum - 1] ?? "";
      if (check.skipComments && lineText.trim().startsWith("//")) continue;
      violations.push(`${relPath}:${lineNum}: ${check.rule} ${check.message}`);
    }
  }
}

function scanCrateRoots(file, relPath) {
  const content = readFileSync(file, "utf8");
  const lines = content.split("\n");
  const exempt = CRATE_ROOT_EXEMPTIONS.get(relPath) ?? new Set();
  for (const { name, index } of collectCrateRoots(content)) {
    const lineNum = lineNumberAt(content, index);
    // Doc comments legitimately name upstream anchors like
    // `crate::commands::misc::npm_package_for`; only executable lines count.
    if ((lines[lineNum - 1] ?? "").trim().startsWith("//")) continue;
    if (ALLOWED_CRATE_ROOTS.has(name)) continue;
    if (exempt.has(name)) {
      usedExemptions.add(`${relPath}:${name}`);
      continue;
    }
    violations.push(
      `${relPath}:${lineNum}: [R1/R2] crate::${name} is not a product layer; reach upstream through crate::compat::ccswitch`,
    );
  }
}

const tauriCheck = {
  rule: "[R1]",
  pattern: TAURI_PATTERN,
  skipComments: true,
  message: "domain must not depend on Tauri",
};
const tauriImportCheck = {
  rule: "[R4]",
  pattern: TAURI_IMPORT_PATTERN,
  message: "only src/native may import @tauri-apps (spec section 21)",
};

for (const dir of [...R1_DIRS, ...R2_DIRS]) {
  const isDomain = dir.endsWith("/domain");
  for (const file of walkDir(dir, [".rs"])) {
    const relPath = toPosix(relative(ROOT, file));
    scanCrateRoots(file, relPath);
    if (isDomain) scan(file, relPath, [tauriCheck]);
  }
}

for (const [relPath, roots] of CRATE_ROOT_EXEMPTIONS) {
  for (const root of roots) {
    if (!usedExemptions.has(`${relPath}:${root}`)) {
      violations.push(
        `${relPath}: [R1/R2-stale] exemption for crate::${root} is no longer used; delete it`,
      );
    }
  }
}

const seenAllowlisted = new Set();
for (const file of walkDir("src", [".ts", ".tsx"])) {
  const relPath = toPosix(relative(ROOT, file));
  if (R4_ALLOWED_DIRS.some((dir) => relPath.startsWith(`${dir}/`))) continue;
  if (R4_ALLOWLIST.has(relPath)) {
    seenAllowlisted.add(relPath);
    continue;
  }
  scan(file, relPath, [tauriImportCheck]);
}

for (const entry of R4_ALLOWLIST) {
  if (!seenAllowlisted.has(entry)) {
    violations.push(
      `${entry}: [R4-stale] allowlisted file no longer exists under src/`,
    );
  }
}

const nativeImportCheck = {
  rule: "[R5]",
  pattern: NATIVE_IMPORT_PATTERN,
  message:
    "pages and the app shell must consume the backend through entities/features hooks (spec section 20)",
};

for (const dir of R5_DIRS) {
  for (const file of walkDir(dir, [".ts", ".tsx"])) {
    scan(file, toPosix(relative(ROOT, file)), [nativeImportCheck]);
  }
}

if (violations.length > 0) {
  console.error(`Architecture boundary violations (${violations.length}):`);
  for (const violation of violations) {
    console.error(`  ${violation}`);
  }
  process.exit(1);
}

console.log("Architecture boundaries OK");
