import assert from "node:assert/strict";
import test from "node:test";
import { verifyWindowsBundleMarkerBuffers } from "./verify-windows-bundle-marker.mjs";

const unknown = Buffer.from("__TAURI_BUNDLE_TYPE_VAR_UNK", "ascii");
const msi = Buffer.from("__TAURI_BUNDLE_TYPE_VAR_MSI", "ascii");
const nsis = Buffer.from("__TAURI_BUNDLE_TYPE_VAR_NSS", "ascii");

function executable(marker) {
  return Buffer.concat([
    Buffer.from("MZ fixture-prefix\0", "ascii"),
    marker,
    Buffer.from("\0fixture-suffix", "ascii"),
  ]);
}

test("accepts an MSI marker written at the clean executable marker offset", () => {
  const evidence = verifyWindowsBundleMarkerBuffers(
    executable(unknown),
    executable(msi),
    "msi",
  );

  assert.equal(evidence.expectedType, "msi");
  assert.equal(evidence.marker, msi.toString("ascii"));
  assert.equal(evidence.markerOffset, "MZ fixture-prefix\0".length);
});

test("accepts an NSIS marker written at the clean executable marker offset", () => {
  assert.doesNotThrow(() =>
    verifyWindowsBundleMarkerBuffers(
      executable(unknown),
      executable(nsis),
      "nsis",
    ),
  );
});

test("rejects a missing, duplicate, unknown, or wrong installer marker", () => {
  assert.throws(
    () =>
      verifyWindowsBundleMarkerBuffers(executable(msi), executable(msi), "msi"),
    /exactly one unknown Tauri bundle marker/,
  );
  assert.throws(
    () =>
      verifyWindowsBundleMarkerBuffers(
        Buffer.concat([executable(unknown), unknown]),
        executable(msi),
        "msi",
      ),
    /found 2/,
  );
  assert.throws(
    () =>
      verifyWindowsBundleMarkerBuffers(
        executable(unknown),
        executable(unknown),
        "msi",
      ),
    /marker mismatch/,
  );
  assert.throws(
    () =>
      verifyWindowsBundleMarkerBuffers(
        executable(unknown),
        executable(nsis),
        "msi",
      ),
    /marker mismatch/,
  );
  assert.throws(
    () =>
      verifyWindowsBundleMarkerBuffers(
        executable(unknown),
        executable(msi),
        "zip",
      ),
    /must be msi or nsis/,
  );
});
