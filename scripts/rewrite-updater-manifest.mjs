#!/usr/bin/env node

import { readFile, readdir, stat, writeFile } from "node:fs/promises";
import { basename, join, resolve } from "node:path";
import { pathToFileURL } from "node:url";
import {
  expectedAssetNames,
  validateLatestManifest,
} from "./assemble-latest-json.mjs";

export const PRODUCT_RELEASE_REPOSITORY = "OnlistTeam/ai-manager";
export const PRODUCT_RELEASE_BASE_URL = "https://dl.aimanager.tools/ai-manager";

const PLATFORM_ARTIFACT = {
  "darwin-aarch64": (version) => `AI-Manager-${version}-macOS-arm64.app.tar.gz`,
  "darwin-x86_64": (version) => `AI-Manager-${version}-macOS-x64.app.tar.gz`,
  "windows-x86_64": (version) => `AI-Manager-${version}-Windows-x64.msi`,
  "linux-x86_64": (version) =>
    `AI-Manager-${version}-Linux-x64.AppImage`,
};

function normalizeReleaseBaseUrl(baseUrl) {
  let parsed;
  try {
    parsed = new URL(baseUrl);
  } catch {
    throw new Error(`invalid release base URL: ${baseUrl}`);
  }

  const normalized = `${parsed.protocol}//${parsed.hostname}${parsed.pathname.replace(/\/+$/, "")}`;
  if (
    parsed.protocol !== "https:" ||
    parsed.username ||
    parsed.password ||
    parsed.search ||
    parsed.hash ||
    parsed.port ||
    normalized !== PRODUCT_RELEASE_BASE_URL
  ) {
    throw new Error(
      "release base URL must be an approved product-owned HTTPS AI Manager path",
    );
  }

  return normalized;
}

async function validateMirrorPayload({ assetsDir, manifest }) {
  const expected = new Set([
    ...expectedAssetNames(manifest.version),
    "latest.json",
  ]);
  const actual = await readdir(assetsDir);

  for (const name of actual) {
    if (!expected.has(name)) {
      throw new Error(`unexpected mirrored release asset: ${name}`);
    }
  }
  for (const name of expected) {
    if (!actual.includes(name)) {
      throw new Error(`missing mirrored release asset: ${name}`);
    }
    if ((await stat(join(assetsDir, name))).size === 0) {
      throw new Error(`mirrored release asset is empty: ${name}`);
    }
  }

  for (const [platform, artifactNameForVersion] of Object.entries(
    PLATFORM_ARTIFACT,
  )) {
    const artifactName = artifactNameForVersion(manifest.version);
    const signature = (
      await readFile(join(assetsDir, `${artifactName}.sig`), "utf8")
    ).trim();
    if (manifest.platforms[platform].signature !== signature) {
      throw new Error(
        `${platform} signature does not match its mirrored signature file`,
      );
    }
  }
}

export async function rewriteUpdaterManifest({
  manifest,
  assetsDir,
  repository = PRODUCT_RELEASE_REPOSITORY,
  tag,
  baseUrl = PRODUCT_RELEASE_BASE_URL,
}) {
  validateLatestManifest(manifest, { repository, tag });
  await validateMirrorPayload({ assetsDir, manifest });
  const normalizedBase = normalizeReleaseBaseUrl(baseUrl);
  const rewritten = structuredClone(manifest);

  for (const [platform, artifactNameForVersion] of Object.entries(
    PLATFORM_ARTIFACT,
  )) {
    const artifactName = artifactNameForVersion(manifest.version);
    rewritten.platforms[platform].url =
      `${normalizedBase}/${tag}/${encodeURIComponent(artifactName)}`;
  }

  return rewritten;
}

function parseArguments(args) {
  const values = new Map();
  const allowed = new Set([
    "input",
    "assets",
    "repository",
    "tag",
    "base-url",
    "output",
  ]);
  for (let index = 0; index < args.length; index += 2) {
    const flag = args[index];
    const value = args[index + 1];
    if (!flag?.startsWith("--") || value === undefined) {
      throw new Error(`invalid argument near ${flag ?? "end of command"}`);
    }
    const name = flag.slice(2);
    if (!allowed.has(name)) {
      throw new Error(`unknown argument --${name}`);
    }
    if (values.has(name)) {
      throw new Error(`duplicate argument --${name}`);
    }
    values.set(name, value);
  }
  for (const required of allowed) {
    if (!values.has(required)) {
      throw new Error(`missing --${required}`);
    }
  }
  return values;
}

async function main() {
  const args = parseArguments(process.argv.slice(2));
  const input = resolve(args.get("input"));
  const output = resolve(args.get("output"));
  const manifest = JSON.parse(await readFile(input, "utf8"));
  const rewritten = await rewriteUpdaterManifest({
    manifest,
    assetsDir: resolve(args.get("assets")),
    repository: args.get("repository"),
    tag: args.get("tag"),
    baseUrl: args.get("base-url"),
  });
  await writeFile(output, `${JSON.stringify(rewritten, null, 2)}\n`);
  console.log(
    `Wrote ${basename(output)} for ${rewritten.version} at ${args.get("base-url")}`,
  );
}

const invokedPath = process.argv[1]
  ? pathToFileURL(resolve(process.argv[1])).href
  : "";
if (import.meta.url === invokedPath) {
  main().catch((error) => {
    console.error(error instanceof Error ? error.message : String(error));
    process.exitCode = 1;
  });
}
