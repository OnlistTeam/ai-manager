import { readFile } from "node:fs/promises";
import { resolve } from "node:path";
import { fileURLToPath } from "node:url";

const UNKNOWN_MARKER = Buffer.from("__TAURI_BUNDLE_TYPE_VAR_UNK", "ascii");
const EXPECTED_MARKERS = Object.freeze({
  msi: Buffer.from("__TAURI_BUNDLE_TYPE_VAR_MSI", "ascii"),
  nsis: Buffer.from("__TAURI_BUNDLE_TYPE_VAR_NSS", "ascii"),
});

function indexesOf(haystack, needle) {
  const indexes = [];
  let offset = 0;
  while ((offset = haystack.indexOf(needle, offset)) >= 0) {
    indexes.push(offset);
    offset += needle.length;
  }
  return indexes;
}

export function verifyWindowsBundleMarkerBuffers(
  cleanExecutable,
  packagedExecutable,
  expectedType,
) {
  const expectedMarker = EXPECTED_MARKERS[expectedType];
  if (!expectedMarker) {
    throw new Error("expected bundle type must be msi or nsis");
  }

  const sourceOffsets = indexesOf(cleanExecutable, UNKNOWN_MARKER);
  if (sourceOffsets.length !== 1) {
    throw new Error(
      `clean executable must contain exactly one unknown Tauri bundle marker; found ${sourceOffsets.length}`,
    );
  }

  const markerOffset = sourceOffsets[0];
  const packagedMarker = packagedExecutable.subarray(
    markerOffset,
    markerOffset + expectedMarker.length,
  );
  if (!packagedMarker.equals(expectedMarker)) {
    throw new Error(
      `packaged executable marker mismatch at byte ${markerOffset}: expected ${expectedMarker.toString("ascii")}`,
    );
  }
  if (indexesOf(packagedExecutable, UNKNOWN_MARKER).length !== 0) {
    throw new Error(
      "packaged executable still contains an unknown Tauri bundle marker",
    );
  }

  return {
    expectedType,
    marker: expectedMarker.toString("ascii"),
    markerOffset,
  };
}

export async function verifyWindowsBundleMarker(
  cleanExecutablePath,
  packagedExecutablePath,
  expectedType,
) {
  const [cleanExecutable, packagedExecutable] = await Promise.all([
    readFile(cleanExecutablePath),
    readFile(packagedExecutablePath),
  ]);
  return verifyWindowsBundleMarkerBuffers(
    cleanExecutable,
    packagedExecutable,
    expectedType,
  );
}

async function main() {
  const [, , cleanExecutablePath, packagedExecutablePath, expectedType] =
    process.argv;
  if (!cleanExecutablePath || !packagedExecutablePath || !expectedType) {
    throw new Error(
      "usage: node scripts/verify-windows-bundle-marker.mjs <clean-exe> <packaged-exe> <msi|nsis>",
    );
  }

  const evidence = await verifyWindowsBundleMarker(
    resolve(cleanExecutablePath),
    resolve(packagedExecutablePath),
    expectedType,
  );
  process.stdout.write(`${JSON.stringify(evidence)}\n`);
}

if (
  process.argv[1] &&
  resolve(process.argv[1]) === fileURLToPath(import.meta.url)
) {
  main().catch((error) => {
    console.error(error instanceof Error ? error.message : error);
    process.exitCode = 1;
  });
}
