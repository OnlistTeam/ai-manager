#!/usr/bin/env node

import { createHash } from "node:crypto";
import { readFile, readdir, writeFile } from "node:fs/promises";
import { basename, join, resolve } from "node:path";
import { pathToFileURL } from "node:url";

const PLATFORM_KEYS = [
  "darwin-aarch64",
  "darwin-x86_64",
  "windows-x86_64",
  "linux-x86_64",
];
const SEMVER_IDENTIFIER = "(?:0|[1-9]\\d*|\\d*[A-Za-z-][0-9A-Za-z-]*)";
const RELEASE_TAG = new RegExp(
  `^v((?:0|[1-9]\\d*)\\.(?:0|[1-9]\\d*)\\.(?:0|[1-9]\\d*)` +
    `(?:-${SEMVER_IDENTIFIER}(?:\\.${SEMVER_IDENTIFIER})*)?` +
    `(?:\\+[0-9A-Za-z-]+(?:\\.[0-9A-Za-z-]+)*)?)$`,
);
const RFC3339 =
  /^\d{4}-\d{2}-\d{2}T\d{2}:\d{2}:\d{2}(?:\.\d+)?(?:Z|[+-]\d{2}:\d{2})$/;

export function releaseNames(version) {
  return {
    macArmDmg: `AI-Manager-${version}-macOS-arm64.dmg`,
    macArmUpdater: `AI-Manager-${version}-macOS-arm64.app.tar.gz`,
    macX64Dmg: `AI-Manager-${version}-macOS-x64.dmg`,
    macX64Updater: `AI-Manager-${version}-macOS-x64.app.tar.gz`,
    windowsMsi: `AI-Manager-${version}-Windows-x64.msi`,
    linuxAppImage: `AI-Manager-${version}-Linux-x64.AppImage`,
    linuxDeb: `AI-Manager-${version}-Linux-x64.deb`,
  };
}

export function parseTag(tag) {
  const match = tag.match(RELEASE_TAG);
  if (!match) {
    throw new Error(`release tag must be v-prefixed SemVer: ${tag}`);
  }
  return match[1];
}

export function validateRepository(repository) {
  if (!/^[A-Za-z0-9_.-]+\/[A-Za-z0-9_.-]+$/.test(repository)) {
    throw new Error(`invalid GitHub repository slug: ${repository}`);
  }
}

function normalizeBase64(value) {
  return value.replace(/\s+/g, "").replace(/=+$/, "");
}

function validateUpdaterSignature(signature, label = "updater signature") {
  if (!signature || !/^[A-Za-z0-9+/=\r\n]+$/.test(signature)) {
    throw new Error(`${label} is missing or is not base64`);
  }

  const compact = signature.replace(/\s+/g, "");
  let decoded;
  try {
    decoded = Buffer.from(compact, "base64");
  } catch {
    throw new Error(`${label} is not valid base64`);
  }
  if (
    decoded.length === 0 ||
    normalizeBase64(decoded.toString("base64")) !== normalizeBase64(compact)
  ) {
    throw new Error(`${label} is not canonical base64`);
  }

  const lines = decoded.toString("utf8").trimEnd().split(/\r?\n/);
  if (
    lines.length !== 4 ||
    lines[0] !== "untrusted comment: signature from tauri secret key" ||
    !lines[2].startsWith("trusted comment: timestamp:") ||
    !/^[A-Za-z0-9+/=]+$/.test(lines[1]) ||
    !/^[A-Za-z0-9+/=]+$/.test(lines[3])
  ) {
    throw new Error(`${label} is not a Tauri minisign signature box`);
  }
}

export function expectedAssetNames(version) {
  const names = releaseNames(version);
  return new Set([
    names.macArmDmg,
    `${names.macArmDmg}.sha256`,
    names.macArmUpdater,
    `${names.macArmUpdater}.sig`,
    names.macX64Dmg,
    `${names.macX64Dmg}.sha256`,
    names.macX64Updater,
    `${names.macX64Updater}.sig`,
    names.windowsMsi,
    `${names.windowsMsi}.sha256`,
    `${names.windowsMsi}.sig`,
    names.linuxAppImage,
    `${names.linuxAppImage}.sha256`,
    // Tauri signs the AppImage in place rather than wrapping it in a tarball,
    // so the installer and the update payload are the same file.
    `${names.linuxAppImage}.sig`,
    names.linuxDeb,
    `${names.linuxDeb}.sha256`,
  ]);
}

/**
 * Reads the published `.sha256` record beside an artifact, checks it really
 * describes that artifact, and recomputes the digest from the bytes on disk.
 * Returns the verified digest so callers can republish it without trusting the
 * record a second time.
 */
export async function readVerifiedChecksum(assetsDir, artifactName) {
  const checksumPath = join(assetsDir, `${artifactName}.sha256`);
  const checksum = (await readFile(checksumPath, "utf8")).trim();
  const match = checksum.match(/^([a-f0-9]{64}) {2}(.+)$/);
  if (!match || match[2] !== artifactName) {
    throw new Error(`invalid checksum record for ${artifactName}`);
  }

  const artifact = await readFile(join(assetsDir, artifactName));
  const actual = createHash("sha256").update(artifact).digest("hex");
  if (actual !== match[1]) {
    throw new Error(`checksum mismatch for ${artifactName}`);
  }
  return actual;
}

async function validateChecksum(assetsDir, artifactName) {
  await readVerifiedChecksum(assetsDir, artifactName);
}

function artifactUrl(repository, tag, name) {
  return `https://github.com/${repository}/releases/download/${tag}/${encodeURIComponent(name)}`;
}

export function validateLatestManifest(manifest, { repository, tag }) {
  validateRepository(repository);
  const version = parseTag(tag);
  if (manifest?.version !== version) {
    throw new Error("latest.json version does not match the release tag");
  }
  if (
    !RFC3339.test(manifest.pub_date ?? "") ||
    Number.isNaN(Date.parse(manifest.pub_date))
  ) {
    throw new Error("latest.json pub_date must be RFC 3339");
  }

  const keys = Object.keys(manifest.platforms ?? {});
  if (
    keys.length !== PLATFORM_KEYS.length ||
    !PLATFORM_KEYS.every((key, index) => keys[index] === key)
  ) {
    throw new Error(
      "latest.json must contain exactly the four release platforms",
    );
  }

  const names = releaseNames(version);
  const expectedArtifacts = {
    "darwin-aarch64": names.macArmUpdater,
    "darwin-x86_64": names.macX64Updater,
    "windows-x86_64": names.windowsMsi,
    "linux-x86_64": names.linuxAppImage,
  };

  for (const key of PLATFORM_KEYS) {
    const entry = manifest.platforms[key];
    validateUpdaterSignature(entry?.signature, `${key} signature`);
    const expectedUrl = artifactUrl(repository, tag, expectedArtifacts[key]);
    if (entry?.url !== expectedUrl) {
      throw new Error(`${key} URL does not belong to the expected repository`);
    }
  }
}

export async function assembleLatestManifest({
  assetsDir,
  repository,
  tag,
  pubDate,
}) {
  validateRepository(repository);
  const version = parseTag(tag);
  const normalizedDate = new Date(pubDate);
  if (Number.isNaN(normalizedDate.getTime())) {
    throw new Error(`invalid publication date: ${pubDate}`);
  }

  const expected = expectedAssetNames(version);
  const actual = await readdir(assetsDir);
  for (const name of actual) {
    if (!expected.has(name)) {
      throw new Error(`unexpected release asset: ${name}`);
    }
  }
  for (const name of expected) {
    if (!actual.includes(name)) {
      throw new Error(`missing release asset: ${name}`);
    }
  }

  const names = releaseNames(version);
  await Promise.all([
    validateChecksum(assetsDir, names.macArmDmg),
    validateChecksum(assetsDir, names.macX64Dmg),
    validateChecksum(assetsDir, names.windowsMsi),
    validateChecksum(assetsDir, names.linuxAppImage),
    validateChecksum(assetsDir, names.linuxDeb),
  ]);

  const signatures = await Promise.all(
    [
      names.macArmUpdater,
      names.macX64Updater,
      names.windowsMsi,
      names.linuxAppImage,
    ].map(async (name) => {
      const signature = (
        await readFile(join(assetsDir, `${name}.sig`), "utf8")
      ).trim();
      validateUpdaterSignature(signature, `updater signature ${name}.sig`);
      return signature;
    }),
  );

  const manifest = {
    version,
    notes: `AI Manager ${tag}`,
    pub_date: normalizedDate.toISOString(),
    platforms: {
      "darwin-aarch64": {
        signature: signatures[0],
        url: artifactUrl(repository, tag, names.macArmUpdater),
      },
      "darwin-x86_64": {
        signature: signatures[1],
        url: artifactUrl(repository, tag, names.macX64Updater),
      },
      "windows-x86_64": {
        signature: signatures[2],
        url: artifactUrl(repository, tag, names.windowsMsi),
      },
      "linux-x86_64": {
        signature: signatures[3],
        url: artifactUrl(repository, tag, names.linuxAppImage),
      },
    },
  };
  validateLatestManifest(manifest, { repository, tag });
  return manifest;
}

function parseArguments(args) {
  const values = new Map();
  const allowed = new Set([
    "assets",
    "repository",
    "tag",
    "pub-date",
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
  for (const required of [
    "assets",
    "repository",
    "tag",
    "pub-date",
    "output",
  ]) {
    if (!values.has(required)) {
      throw new Error(`missing --${required}`);
    }
  }
  return values;
}

async function main() {
  const args = parseArguments(process.argv.slice(2));
  const manifest = await assembleLatestManifest({
    assetsDir: resolve(args.get("assets")),
    repository: args.get("repository"),
    tag: args.get("tag"),
    pubDate: args.get("pub-date"),
  });
  const output = resolve(args.get("output"));
  await writeFile(output, `${JSON.stringify(manifest, null, 2)}\n`);
  console.log(`Wrote ${basename(output)} for ${manifest.version}`);
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
