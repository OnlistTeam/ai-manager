import assert from "node:assert/strict";
import { mkdtemp, readFile, writeFile } from "node:fs/promises";
import { tmpdir } from "node:os";
import { join } from "node:path";
import test from "node:test";
import { captureWindowsQaSigningInput } from "./capture-windows-qa-signing-input.mjs";

async function fixture(name, body = "fixture") {
  const root = await mkdtemp(
    join(tmpdir(), "ai-manager-windows-capture-test-"),
  );
  const source = join(root, name);
  const capture = join(root, "capture");
  await writeFile(
    source,
    Buffer.concat([Buffer.from("MZ"), Buffer.from(body)]),
  );
  return { source, capture };
}

test("captures the exact Tauri-patched application with explicit unsigned evidence", async () => {
  const { source, capture } = await fixture(
    "ai-manager.exe",
    "__TAURI_BUNDLE_TYPE_VAR_NSS",
  );

  const result = await captureWindowsQaSigningInput(source, capture);
  assert.equal(result.evidence.captureRole, "tauri-patched-application");
  assert.equal(result.evidence.capturedName, "packaged-ai-manager.exe");
  assert.equal(result.evidence.authenticodeState, "not-verified-qa-capture");
  assert.deepEqual(await readFile(result.destination), await readFile(source));

  const persisted = JSON.parse(await readFile(result.evidencePath, "utf8"));
  assert.equal(persisted.sha256, result.evidence.sha256);
  assert.equal(persisted.size, result.evidence.size);
});

test("captures an NSIS installer separately from its packaged application", async () => {
  const { source, capture } = await fixture("AI Manager_0.1.0_x64-setup.exe");

  const result = await captureWindowsQaSigningInput(source, capture);
  assert.equal(result.evidence.captureRole, "unsigned-nsis-installer");
  assert.equal(result.evidence.capturedName, "unsigned-nsis-installer.exe");
});

test("records Tauri NSIS plugin inputs without treating them as product executables", async () => {
  const { source, capture } = await fixture("NSISdl.dll");

  const result = await captureWindowsQaSigningInput(source, capture);
  assert.equal(result.evidence.captureRole, "unsigned-nsis-plugin");
  assert.equal(result.evidence.capturedName, "unsigned-nsis-plugin-nsisdl.dll");
});

test("rejects unknown inputs, non-PE files, and ambiguous overwrite attempts", async () => {
  const unknown = await fixture("another.exe");
  await assert.rejects(
    captureWindowsQaSigningInput(unknown.source, unknown.capture),
    /unexpected Windows QA signing input/,
  );

  const notPe = await fixture("ai-manager.exe");
  await writeFile(notPe.source, "not a PE file");
  await assert.rejects(
    captureWindowsQaSigningInput(notPe.source, notPe.capture),
    /not a PE executable/,
  );

  const duplicate = await fixture("ai-manager.exe");
  await captureWindowsQaSigningInput(duplicate.source, duplicate.capture);
  await assert.rejects(
    captureWindowsQaSigningInput(duplicate.source, duplicate.capture),
    /EEXIST/,
  );
});

test("records the extensionless temporary NSIS uninstaller signing input", async () => {
  const { source, capture } = await fixture("makensisqL64NS");

  const result = await captureWindowsQaSigningInput(source, capture);
  assert.equal(result.evidence.captureRole, "unsigned-nsis-uninstaller-stub");
  assert.equal(
    result.evidence.capturedName,
    "unsigned-nsis-uninstaller-stub.exe",
  );
});
