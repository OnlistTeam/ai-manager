import { readFile } from "node:fs/promises";
import { resolve } from "node:path";
import { fileURLToPath } from "node:url";

const DOS_SIGNATURE = 0x5a4d;
const PE_SIGNATURE = 0x00004550;
const PE32_MAGIC = 0x10b;
const PE32_PLUS_MAGIC = 0x20b;
const SECURITY_DIRECTORY_INDEX = 4;

function requireRange(bytes, offset, length, label) {
  if (offset < 0 || length < 0 || offset + length > bytes.length) {
    throw new Error(`PE ${label} is outside the file`);
  }
}

export function inspectPeSecurityDirectory(bytes) {
  requireRange(bytes, 0, 0x40, "DOS header");
  if (bytes.readUInt16LE(0) !== DOS_SIGNATURE) {
    throw new Error("file does not have an MZ header");
  }

  const peOffset = bytes.readUInt32LE(0x3c);
  requireRange(bytes, peOffset, 24, "header");
  if (bytes.readUInt32LE(peOffset) !== PE_SIGNATURE) {
    throw new Error("file does not have a PE signature");
  }

  const fileHeaderOffset = peOffset + 4;
  const optionalHeaderSize = bytes.readUInt16LE(fileHeaderOffset + 16);
  const optionalHeaderOffset = fileHeaderOffset + 20;
  requireRange(
    bytes,
    optionalHeaderOffset,
    optionalHeaderSize,
    "optional header",
  );

  const magic = bytes.readUInt16LE(optionalHeaderOffset);
  const peKind =
    magic === PE32_MAGIC ? "PE32" : magic === PE32_PLUS_MAGIC ? "PE32+" : null;
  if (!peKind)
    throw new Error(`unsupported PE optional-header magic: ${magic}`);

  const directoryCountOffset =
    optionalHeaderOffset + (peKind === "PE32" ? 92 : 108);
  const directoryStart = optionalHeaderOffset + (peKind === "PE32" ? 96 : 112);
  requireRange(bytes, directoryCountOffset, 4, "data-directory count");
  const directoryCount = bytes.readUInt32LE(directoryCountOffset);
  if (directoryCount <= SECURITY_DIRECTORY_INDEX) {
    throw new Error("PE optional header has no security directory entry");
  }

  const securityEntryOffset = directoryStart + SECURITY_DIRECTORY_INDEX * 8;
  if (securityEntryOffset + 8 > optionalHeaderOffset + optionalHeaderSize) {
    throw new Error("PE security directory is outside the optional header");
  }

  const certificateTableOffset = bytes.readUInt32LE(securityEntryOffset);
  const certificateTableSize = bytes.readUInt32LE(securityEntryOffset + 4);
  if ((certificateTableOffset === 0) !== (certificateTableSize === 0)) {
    throw new Error(
      "PE security directory has an incomplete certificate table",
    );
  }
  if (certificateTableSize > 0) {
    requireRange(
      bytes,
      certificateTableOffset,
      certificateTableSize,
      "certificate table",
    );
  }

  return {
    peKind,
    certificateTableOffset,
    certificateTableSize,
    hasEmbeddedAuthenticode: certificateTableSize > 0,
  };
}

export async function inspectPeSecurityDirectoryFile(filePath) {
  return inspectPeSecurityDirectory(await readFile(filePath));
}

async function main() {
  const filePath = process.argv[2];
  if (!filePath) {
    throw new Error(
      "usage: node scripts/inspect-pe-security-directory.mjs <PE-file>",
    );
  }

  const evidence = await inspectPeSecurityDirectoryFile(resolve(filePath));
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
