import assert from "node:assert/strict";
import { createHash } from "node:crypto";
import { mkdtemp, writeFile } from "node:fs/promises";
import { tmpdir } from "node:os";
import { join } from "node:path";
import test from "node:test";
import {
  buildDownloadManifest,
  validateDownloadManifest,
} from "./build-download-manifest.mjs";

const VERSION = "0.1.0";
const TAG = `v${VERSION}`;
const REPOSITORY = "OnlistTeam/ai-manager";
const BASE_URL = "https://dl.aimanager.tools/ai-manager";
const PUBLISHED_AT = "2026-08-20T12:00:00.000Z";

const INSTALLER_NAMES = [
  `AI-Manager-${VERSION}-macOS-arm64.dmg`,
  `AI-Manager-${VERSION}-macOS-x64.dmg`,
  `AI-Manager-${VERSION}-Windows-x64.msi`,
  `AI-Manager-${VERSION}-Linux-x64.AppImage`,
  `AI-Manager-${VERSION}-Linux-x64.deb`,
];

/** A release directory whose `.sha256` records actually describe the bytes. */
async function stageAssets(overrides = {}) {
  const dir = await mkdtemp(join(tmpdir(), "download-manifest-"));
  for (const name of INSTALLER_NAMES) {
    const body = overrides.body?.(name) ?? `payload for ${name}`;
    await writeFile(join(dir, name), body);
    const digest =
      overrides.digest?.(name) ??
      createHash("sha256").update(body).digest("hex");
    await writeFile(join(dir, `${name}.sha256`), `${digest}  ${name}\n`);
  }
  return dir;
}

function build(assetsDir, extra = {}) {
  return buildDownloadManifest({
    assetsDir,
    repository: REPOSITORY,
    tag: TAG,
    baseUrl: BASE_URL,
    channel: "stable",
    publishedAt: PUBLISHED_AT,
    ...extra,
  });
}

test("the manifest lists every installer with a size, a digest and an own-host URL", async () => {
  const manifest = await build(await stageAssets());

  assert.equal(manifest.version, VERSION);
  assert.equal(manifest.tag, TAG);
  assert.equal(manifest.channel, "stable");
  assert.equal(manifest.publishedAt, PUBLISHED_AT);
  assert.equal(
    manifest.releaseNotes,
    `https://github.com/${REPOSITORY}/releases/tag/${TAG}`,
  );
  assert.equal(manifest.files.length, 5);

  for (const file of manifest.files) {
    assert.ok(file.size > 0, `${file.name} has a size`);
    assert.match(file.sha256, /^[a-f0-9]{64}$/);
    assert.equal(file.url, `${BASE_URL}/${TAG}/${file.name}`);
  }

  assert.deepEqual(
    manifest.files.map((file) => [file.platform, file.arch, file.kind]),
    [
      ["macos", "arm64", "dmg"],
      ["macos", "x64", "dmg"],
      ["windows", "x64", "msi"],
      ["linux", "x64", "appimage"],
      ["linux", "x64", "deb"],
    ],
  );

  validateDownloadManifest(manifest, { repository: REPOSITORY, tag: TAG });
});

test("a digest that does not describe the file is refused", async () => {
  const dir = await stageAssets({
    digest: (name) => (name.endsWith(".deb") ? "0".repeat(64) : undefined),
  });
  await assert.rejects(build(dir), /checksum mismatch/);
});

test("a missing installer is refused rather than silently dropped", async () => {
  const dir = await mkdtemp(join(tmpdir(), "download-manifest-"));
  await assert.rejects(build(dir));
});

test("an empty installer is refused", async () => {
  const dir = await stageAssets({ body: () => "" });
  await assert.rejects(build(dir), /missing or empty/);
});

test("the manifest may only point at the product's own distribution host", async () => {
  const dir = await stageAssets();
  await assert.rejects(
    build(dir, { baseUrl: "https://example.com/ai-manager" }),
    /approved product-owned/,
  );
  await assert.rejects(
    build(dir, { baseUrl: "http://dl.aimanager.tools/ai-manager" }),
    /approved product-owned/,
  );
});

test("an unknown channel is refused", async () => {
  const dir = await stageAssets();
  await assert.rejects(
    build(dir, { channel: "nightly" }),
    /unknown release channel/,
  );
});

test("validation rejects a manifest whose URLs were rewritten after it was built", async () => {
  const manifest = await build(await stageAssets());
  manifest.files[0].url = `https://github.com/${REPOSITORY}/releases/download/${TAG}/${manifest.files[0].name}`;
  assert.throws(
    () =>
      validateDownloadManifest(manifest, { repository: REPOSITORY, tag: TAG }),
    /does not point at this release on the product's own host/,
  );
});

test("validation rejects a manifest that lost an installer", async () => {
  const manifest = await build(await stageAssets());
  manifest.files.pop();
  assert.throws(
    () =>
      validateDownloadManifest(manifest, { repository: REPOSITORY, tag: TAG }),
    /exactly 5 installers/,
  );
});

test("validation rejects a manifest built for a different release", async () => {
  const manifest = await build(await stageAssets());
  assert.throws(
    () =>
      validateDownloadManifest(manifest, {
        repository: REPOSITORY,
        tag: "v9.9.9",
      }),
    /does not match the release tag/,
  );
});
