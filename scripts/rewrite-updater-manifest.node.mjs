import assert from "node:assert/strict";
import { execFile } from "node:child_process";
import { mkdtemp, readFile, writeFile } from "node:fs/promises";
import { tmpdir } from "node:os";
import { join } from "node:path";
import test from "node:test";
import { fileURLToPath } from "node:url";
import { promisify } from "node:util";
import { expectedAssetNames } from "./assemble-latest-json.mjs";
import {
  PRODUCT_RELEASE_BASE_URL,
  PRODUCT_RELEASE_REPOSITORY,
  rewriteUpdaterManifest,
} from "./rewrite-updater-manifest.mjs";

const VERSION = "0.1.0";
const TAG = `v${VERSION}`;
const SIGNATURE = Buffer.from(
  [
    "untrusted comment: signature from tauri secret key",
    "RUTESTSIGNATUREPAYLOAD0123456789abcdefghijklmnop",
    "trusted comment: timestamp:1787227200\tfile:test",
    "RUTESTGLOBALPAYLOAD0123456789abcdefghijklmnopqr",
    "",
  ].join("\n"),
).toString("base64");
const execFileAsync = promisify(execFile);

function sourceUrl(name) {
  return `https://github.com/${PRODUCT_RELEASE_REPOSITORY}/releases/download/${TAG}/${encodeURIComponent(name)}`;
}

function manifest() {
  return {
    version: VERSION,
    notes: `AI Manager ${TAG}`,
    pub_date: "2026-08-25T12:00:00.000Z",
    platforms: {
      "darwin-aarch64": {
        signature: SIGNATURE,
        url: sourceUrl(`AI-Manager-${VERSION}-macOS-arm64.app.tar.gz`),
      },
      "darwin-x86_64": {
        signature: SIGNATURE,
        url: sourceUrl(`AI-Manager-${VERSION}-macOS-x64.app.tar.gz`),
      },
      "windows-x86_64": {
        signature: SIGNATURE,
        url: sourceUrl(`AI-Manager-${VERSION}-Windows-x64.msi`),
      },
      "linux-x86_64": {
        signature: SIGNATURE,
        url: sourceUrl(`AI-Manager-${VERSION}-Linux-x64.AppImage`),
      },
    },
  };
}

async function fixture() {
  const assetsDir = await mkdtemp(join(tmpdir(), "ai-manager-r2-mirror-"));
  for (const name of expectedAssetNames(VERSION)) {
    await writeFile(
      join(assetsDir, name),
      name.endsWith(".sig") ? SIGNATURE : `fixture:${name}`,
    );
  }
  const latest = manifest();
  await writeFile(
    join(assetsDir, "latest.json"),
    `${JSON.stringify(latest, null, 2)}\n`,
  );
  return { assetsDir, latest };
}

test("rewrites only URLs and preserves every signed manifest field", async () => {
  const { assetsDir, latest } = await fixture();
  const rewritten = await rewriteUpdaterManifest({
    manifest: latest,
    assetsDir,
    tag: TAG,
  });

  assert.equal(rewritten.version, latest.version);
  assert.equal(rewritten.notes, latest.notes);
  assert.equal(rewritten.pub_date, latest.pub_date);
  for (const [platform, entry] of Object.entries(rewritten.platforms)) {
    assert.equal(entry.signature, latest.platforms[platform].signature);
    assert.match(
      entry.url,
      /^https:\/\/dl\.aimanager\.tools\/ai-manager\/v0\.1\.0\//,
    );
    assert.doesNotMatch(entry.url, /github\.com/);
  }
});

test("payload drift, signature drift, and non-product mirror URLs fail closed", async () => {
  const extra = await fixture();
  await writeFile(join(extra.assetsDir, "unexpected.exe"), "unexpected");
  await assert.rejects(
    rewriteUpdaterManifest({
      manifest: extra.latest,
      assetsDir: extra.assetsDir,
      tag: TAG,
    }),
    /unexpected mirrored release asset/,
  );

  const signatureDrift = await fixture();
  await writeFile(
    join(signatureDrift.assetsDir, `AI-Manager-${VERSION}-Windows-x64.msi.sig`),
    `${SIGNATURE}drift`,
  );
  await assert.rejects(
    rewriteUpdaterManifest({
      manifest: signatureDrift.latest,
      assetsDir: signatureDrift.assetsDir,
      tag: TAG,
    }),
    /signature does not match/,
  );

  const wrongHost = await fixture();
  await assert.rejects(
    rewriteUpdaterManifest({
      manifest: wrongHost.latest,
      assetsDir: wrongHost.assetsDir,
      tag: TAG,
      baseUrl: "https://dl.aimanager.tools.example/ai-manager",
    }),
    /approved product-owned HTTPS AI Manager path/,
  );

  for (const baseUrl of [
    "https://dl.aimanager.tools/another-product",
    "https://downloads.aimanager.tools/ai-manager",
    "http://dl.aimanager.tools/ai-manager",
    "https://dl.aimanager.tools:8443/ai-manager",
    "https://dl.aimanager.tools/ai-manager?mirror=backup",
  ]) {
    const rejected = await fixture();
    await assert.rejects(
      rewriteUpdaterManifest({
        manifest: rejected.latest,
        assetsDir: rejected.assetsDir,
        tag: TAG,
        baseUrl,
      }),
      /approved product-owned HTTPS AI Manager path/,
    );
  }
});

test("the command writes a validated mirror manifest with a trailing newline", async () => {
  const { assetsDir } = await fixture();
  const output = join(assetsDir, "latest-r2.json");
  await execFileAsync(process.execPath, [
    fileURLToPath(new URL("rewrite-updater-manifest.mjs", import.meta.url)),
    "--input",
    join(assetsDir, "latest.json"),
    "--assets",
    assetsDir,
    "--repository",
    PRODUCT_RELEASE_REPOSITORY,
    "--tag",
    TAG,
    "--base-url",
    PRODUCT_RELEASE_BASE_URL,
    "--output",
    output,
  ]);

  const rendered = await readFile(output, "utf8");
  assert(rendered.endsWith("\n"));
  assert.equal(
    JSON.parse(rendered).platforms["darwin-aarch64"].url,
    `${PRODUCT_RELEASE_BASE_URL}/${TAG}/AI-Manager-${VERSION}-macOS-arm64.app.tar.gz`,
  );
});
