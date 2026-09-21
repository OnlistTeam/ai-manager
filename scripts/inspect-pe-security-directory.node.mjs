import assert from "node:assert/strict";
import test from "node:test";
import { inspectPeSecurityDirectory } from "./inspect-pe-security-directory.mjs";

function executable({
  pe32Plus = true,
  certificateOffset = 0,
  certificateSize = 0,
} = {}) {
  const peOffset = 0x80;
  const optionalHeaderSize = pe32Plus ? 240 : 224;
  const optionalHeaderOffset = peOffset + 24;
  const minimumSize = optionalHeaderOffset + optionalHeaderSize;
  const size = Math.max(minimumSize, certificateOffset + certificateSize);
  const bytes = Buffer.alloc(size);
  bytes.writeUInt16LE(0x5a4d, 0);
  bytes.writeUInt32LE(peOffset, 0x3c);
  bytes.writeUInt32LE(0x00004550, peOffset);
  bytes.writeUInt16LE(optionalHeaderSize, peOffset + 4 + 16);
  bytes.writeUInt16LE(pe32Plus ? 0x20b : 0x10b, optionalHeaderOffset);

  const directoryCountOffset = optionalHeaderOffset + (pe32Plus ? 108 : 92);
  const directoryStart = optionalHeaderOffset + (pe32Plus ? 112 : 96);
  bytes.writeUInt32LE(16, directoryCountOffset);
  const securityEntryOffset = directoryStart + 4 * 8;
  bytes.writeUInt32LE(certificateOffset, securityEntryOffset);
  bytes.writeUInt32LE(certificateSize, securityEntryOffset + 4);
  return bytes;
}

test("reports an absent Authenticode table for unsigned PE32 and PE32+ files", () => {
  assert.deepEqual(inspectPeSecurityDirectory(executable()), {
    peKind: "PE32+",
    certificateTableOffset: 0,
    certificateTableSize: 0,
    hasEmbeddedAuthenticode: false,
  });
  assert.equal(
    inspectPeSecurityDirectory(executable({ pe32Plus: false })).peKind,
    "PE32",
  );
});

test("reports a bounded embedded Authenticode certificate table", () => {
  const evidence = inspectPeSecurityDirectory(
    executable({ certificateOffset: 0x280, certificateSize: 24 }),
  );
  assert.deepEqual(evidence, {
    peKind: "PE32+",
    certificateTableOffset: 0x280,
    certificateTableSize: 24,
    hasEmbeddedAuthenticode: true,
  });
});

test("rejects malformed headers and inconsistent certificate table metadata", () => {
  assert.throws(
    () => inspectPeSecurityDirectory(Buffer.alloc(96)),
    /MZ header/,
  );

  const invalidPe = executable();
  invalidPe.writeUInt32LE(0, 0x80);
  assert.throws(() => inspectPeSecurityDirectory(invalidPe), /PE signature/);

  assert.throws(
    () =>
      inspectPeSecurityDirectory(
        executable({ certificateOffset: 0x280, certificateSize: 0 }),
      ),
    /incomplete certificate table/,
  );

  assert.throws(
    () =>
      inspectPeSecurityDirectory(
        executable({ certificateOffset: 0x280, certificateSize: 24 }).subarray(
          0,
          0x280,
        ),
      ),
    /certificate table.*outside/,
  );
});
