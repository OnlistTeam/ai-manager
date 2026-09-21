#!/usr/bin/env node

/*
 * Builds the manifest the download page reads.
 *
 * The updater manifest (`latest.json`) cannot serve this purpose: it points at
 * update payloads rather than installers, it has no `.dmg` or `.deb` at all,
 * and it carries no file sizes. Publishing a second, smaller manifest beside it
 * lets the download page answer "what is newest, how big is it, and where do I
 * get it" without ever contacting GitHub, which is the whole point of mirroring
 * the release to R2.
 *
 * Every digest here is recomputed from the bytes on disk rather than copied out
 * of the published `.sha256` record, so a manifest can never promise a checksum
 * the file does not have.
 */

import { stat, writeFile } from "node:fs/promises";
import { basename, join, resolve } from "node:path";
import { pathToFileURL } from "node:url";
import {
  parseTag,
  readVerifiedChecksum,
  releaseNames,
  validateRepository,
} from "./assemble-latest-json.mjs";
import { PRODUCT_RELEASE_BASE_URL } from "./rewrite-updater-manifest.mjs";

const CHANNELS = new Set(["stable", "staging"]);

/**
 * The installers, in the order the page lists them when it cannot tell what the
 * visitor is running. `kind` is what the file is, not what it is called: the
 * page turns it into a sentence in the reader's own language.
 */
const INSTALLERS = [
  { key: "macArmDmg", platform: "macos", arch: "arm64", kind: "dmg" },
  { key: "macX64Dmg", platform: "macos", arch: "x64", kind: "dmg" },
  { key: "windowsMsi", platform: "windows", arch: "x64", kind: "msi" },
  { key: "linuxAppImage", platform: "linux", arch: "x64", kind: "appimage" },
  { key: "linuxDeb", platform: "linux", arch: "x64", kind: "deb" },
];

function normalizeBaseUrl(baseUrl) {
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

export async function buildDownloadManifest({
  assetsDir,
  repository,
  tag,
  baseUrl,
  channel,
  publishedAt,
}) {
  validateRepository(repository);
  const version = parseTag(tag);
  const base = normalizeBaseUrl(baseUrl);

  if (!CHANNELS.has(channel)) {
    throw new Error(`unknown release channel: ${channel}`);
  }

  const published = new Date(publishedAt);
  if (Number.isNaN(published.getTime())) {
    throw new Error(`invalid publication date: ${publishedAt}`);
  }

  const names = releaseNames(version);
  const files = [];
  for (const installer of INSTALLERS) {
    const name = names[installer.key];
    const path = join(assetsDir, name);
    const info = await stat(path);
    if (!info.isFile() || info.size === 0) {
      throw new Error(`release asset is missing or empty: ${name}`);
    }
    files.push({
      platform: installer.platform,
      arch: installer.arch,
      kind: installer.kind,
      name,
      size: info.size,
      sha256: await readVerifiedChecksum(assetsDir, name),
      url: `${base}/${tag}/${encodeURIComponent(name)}`,
    });
  }

  return {
    version,
    tag,
    channel,
    publishedAt: published.toISOString(),
    releaseNotes: `https://github.com/${repository}/releases/tag/${encodeURIComponent(tag)}`,
    files,
  };
}

/**
 * Re-read before publishing: the page trusts this file completely, so anything
 * that reaches R2 has to survive the same checks a reader would want applied.
 */
export function validateDownloadManifest(manifest, { repository, tag }) {
  validateRepository(repository);
  const version = parseTag(tag);
  if (manifest?.version !== version) {
    throw new Error("download manifest version does not match the release tag");
  }
  if (manifest.tag !== tag) {
    throw new Error("download manifest tag does not match the release tag");
  }
  if (!CHANNELS.has(manifest.channel)) {
    throw new Error("download manifest has no known release channel");
  }
  if (
    !Array.isArray(manifest.files) ||
    manifest.files.length !== INSTALLERS.length
  ) {
    throw new Error(
      `download manifest must list exactly ${INSTALLERS.length} installers`,
    );
  }
  for (const [index, file] of manifest.files.entries()) {
    const expected = INSTALLERS[index];
    if (
      file.platform !== expected.platform ||
      file.arch !== expected.arch ||
      file.kind !== expected.kind
    ) {
      throw new Error(`download manifest entry ${index} is out of order`);
    }
    if (!Number.isInteger(file.size) || file.size <= 0) {
      throw new Error(`download manifest entry ${file.name} has no size`);
    }
    if (!/^[a-f0-9]{64}$/.test(file.sha256)) {
      throw new Error(`download manifest entry ${file.name} has no digest`);
    }
    if (
      !file.url.startsWith(`${PRODUCT_RELEASE_BASE_URL}/${tag}/`) ||
      decodeURIComponent(basename(file.url)) !== file.name
    ) {
      throw new Error(
        `download manifest entry ${file.name} does not point at this release on the product's own host`,
      );
    }
  }
}

function parseArguments(argv) {
  const allowed = new Set([
    "assets",
    "repository",
    "tag",
    "base-url",
    "channel",
    "published-at",
    "output",
  ]);
  const values = new Map();
  for (let index = 0; index < argv.length; index += 2) {
    const flag = argv[index];
    const value = argv[index + 1];
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
  const output = resolve(args.get("output"));
  const manifest = await buildDownloadManifest({
    assetsDir: resolve(args.get("assets")),
    repository: args.get("repository"),
    tag: args.get("tag"),
    baseUrl: args.get("base-url"),
    channel: args.get("channel"),
    publishedAt: args.get("published-at"),
  });
  validateDownloadManifest(manifest, {
    repository: args.get("repository"),
    tag: args.get("tag"),
  });
  await writeFile(output, `${JSON.stringify(manifest, null, 2)}\n`);
  console.log(
    `Wrote ${basename(output)} for ${manifest.version} on the ${manifest.channel} channel`,
  );
}

const invokedPath = process.argv[1]
  ? pathToFileURL(resolve(process.argv[1])).href
  : "";
if (import.meta.url === invokedPath) {
  main().catch((error) => {
    console.error(error.message);
    process.exit(1);
  });
}
