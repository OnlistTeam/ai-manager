import assert from "node:assert/strict";
import { execFile } from "node:child_process";
import { createHash } from "node:crypto";
import { mkdtemp, readFile, writeFile } from "node:fs/promises";
import { tmpdir } from "node:os";
import { join } from "node:path";
import test from "node:test";
import { fileURLToPath } from "node:url";
import { promisify } from "node:util";
import {
  assembleLatestManifest,
  validateLatestManifest,
} from "./assemble-latest-json.mjs";

const VERSION = "0.1.0";
const TAG = `v${VERSION}`;
const REPOSITORY = "OnlistTeam/ai-manager";
const PUB_DATE = "2026-08-20T12:00:00.000Z";
const execFileAsync = promisify(execFile);
const SIGNATURE = Buffer.from(
  [
    "untrusted comment: signature from tauri secret key",
    "RUTESTSIGNATUREPAYLOAD0123456789abcdefghijklmnop",
    "trusted comment: timestamp:1787227200\tfile:test",
    "RUTESTGLOBALPAYLOAD0123456789abcdefghijklmnopqr",
    "",
  ].join("\n"),
).toString("base64");

const requiredNames = [
  `AI-Manager-${VERSION}-macOS-arm64.dmg`,
  `AI-Manager-${VERSION}-macOS-arm64.dmg.sha256`,
  `AI-Manager-${VERSION}-macOS-arm64.app.tar.gz`,
  `AI-Manager-${VERSION}-macOS-arm64.app.tar.gz.sig`,
  `AI-Manager-${VERSION}-macOS-x64.dmg`,
  `AI-Manager-${VERSION}-macOS-x64.dmg.sha256`,
  `AI-Manager-${VERSION}-macOS-x64.app.tar.gz`,
  `AI-Manager-${VERSION}-macOS-x64.app.tar.gz.sig`,
  `AI-Manager-${VERSION}-Windows-x64.msi`,
  `AI-Manager-${VERSION}-Windows-x64.msi.sha256`,
  `AI-Manager-${VERSION}-Windows-x64.msi.sig`,
  `AI-Manager-${VERSION}-Linux-x64.AppImage`,
  `AI-Manager-${VERSION}-Linux-x64.AppImage.sha256`,
  `AI-Manager-${VERSION}-Linux-x64.deb`,
  `AI-Manager-${VERSION}-Linux-x64.deb.sha256`,
  `AI-Manager-${VERSION}-Linux-x64.AppImage.sig`,
];

async function fixture() {
  const assetsDir = await mkdtemp(join(tmpdir(), "ai-manager-release-"));
  for (const name of requiredNames.filter(
    (candidate) => !candidate.endsWith(".sha256"),
  )) {
    await writeFile(
      join(assetsDir, name),
      name.endsWith(".sig") ? SIGNATURE : `fixture:${name}`,
    );
  }
  for (const name of requiredNames.filter((candidate) =>
    candidate.endsWith(".sha256"),
  )) {
    const artifactName = name.slice(0, -".sha256".length);
    const artifact = await readFile(join(assetsDir, artifactName));
    const digest = createHash("sha256").update(artifact).digest("hex");
    await writeFile(join(assetsDir, name), `${digest}  ${artifactName}\n`);
  }
  return assetsDir;
}

test("assembles the exact four-platform latest.json contract", async () => {
  const assetsDir = await fixture();
  const manifest = await assembleLatestManifest({
    assetsDir,
    repository: REPOSITORY,
    tag: TAG,
    pubDate: PUB_DATE,
  });

  assert.equal(manifest.version, VERSION);
  assert.equal(manifest.pub_date, PUB_DATE);
  assert.deepEqual(Object.keys(manifest.platforms), [
    "darwin-aarch64",
    "darwin-x86_64",
    "windows-x86_64",
    "linux-x86_64",
  ]);
  assert.match(
    manifest.platforms["darwin-aarch64"].url,
    new RegExp(`${REPOSITORY}/releases/download/${TAG}/`),
  );
  assert.match(
    manifest.platforms["darwin-aarch64"].url,
    /macOS-arm64\.app\.tar\.gz$/,
  );
  assert.match(
    manifest.platforms["darwin-x86_64"].url,
    /macOS-x64\.app\.tar\.gz$/,
  );
  assert.match(manifest.platforms["windows-x86_64"].url, /Windows-x64\.msi$/);
  assert.equal(manifest.platforms["windows-x86_64"].signature, SIGNATURE);
  assert.match(
    manifest.platforms["linux-x86_64"].url,
    /Linux-x64\.AppImage$/,
  );
  assert.equal(manifest.platforms["linux-x86_64"].signature, SIGNATURE);
  assert.doesNotThrow(() =>
    validateLatestManifest(manifest, {
      repository: REPOSITORY,
      tag: TAG,
    }),
  );
});

test("missing, malformed, and extra release assets fail closed", async () => {
  const missingDir = await fixture();
  await writeFile(
    join(missingDir, `AI-Manager-${VERSION}-Windows-x64.msi.sig`),
    "",
  );
  await assert.rejects(
    assembleLatestManifest({
      assetsDir: missingDir,
      repository: REPOSITORY,
      tag: TAG,
      pubDate: PUB_DATE,
    }),
    /signature/i,
  );

  const malformedDir = await fixture();
  await writeFile(
    join(malformedDir, `AI-Manager-${VERSION}-macOS-x64.app.tar.gz.sig`),
    "not-base64",
  );
  await assert.rejects(
    assembleLatestManifest({
      assetsDir: malformedDir,
      repository: REPOSITORY,
      tag: TAG,
      pubDate: PUB_DATE,
    }),
    /signature/i,
  );

  const extraDir = await fixture();
  await writeFile(
    join(extraDir, `AI-Manager-${VERSION}-Linux-x64.rpm`),
    "unexpected",
  );
  await assert.rejects(
    assembleLatestManifest({
      assetsDir: extraDir,
      repository: REPOSITORY,
      tag: TAG,
      pubDate: PUB_DATE,
    }),
    /unexpected release asset/i,
  );
});

test("manifest validation rejects an artifact URL from another repository", async () => {
  const assetsDir = await fixture();
  const manifest = await assembleLatestManifest({
    assetsDir,
    repository: REPOSITORY,
    tag: TAG,
    pubDate: PUB_DATE,
  });
  manifest.platforms["windows-x86_64"].url = manifest.platforms[
    "windows-x86_64"
  ].url.replace(REPOSITORY, "someone-else/ai-manager");

  assert.throws(
    () =>
      validateLatestManifest(manifest, {
        repository: REPOSITORY,
        tag: TAG,
      }),
    /repository/i,
  );
});

test("release metadata rejects malformed SemVer tags and non-RFC3339 dates", async () => {
  const assetsDir = await fixture();
  await assert.rejects(
    assembleLatestManifest({
      assetsDir,
      repository: REPOSITORY,
      tag: "v0.1.0-..",
      pubDate: PUB_DATE,
    }),
    /SemVer/,
  );

  const manifest = await assembleLatestManifest({
    assetsDir,
    repository: REPOSITORY,
    tag: TAG,
    pubDate: PUB_DATE,
  });
  manifest.pub_date = "August 20, 2026";
  assert.throws(
    () =>
      validateLatestManifest(manifest, { repository: REPOSITORY, tag: TAG }),
    /RFC 3339/,
  );
});

test("the command writes validated JSON with a trailing newline", async () => {
  const assetsDir = await fixture();
  const output = join(assetsDir, "latest.json");
  await execFileAsync(process.execPath, [
    fileURLToPath(new URL("assemble-latest-json.mjs", import.meta.url)),
    "--assets",
    assetsDir,
    "--repository",
    REPOSITORY,
    "--tag",
    TAG,
    "--pub-date",
    PUB_DATE,
    "--output",
    output,
  ]);

  const rendered = await readFile(output, "utf8");
  assert(rendered.endsWith("\n"));
  assert.doesNotThrow(() =>
    validateLatestManifest(JSON.parse(rendered), {
      repository: REPOSITORY,
      tag: TAG,
    }),
  );
});
