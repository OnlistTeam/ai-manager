import {
  constants,
  copyFile,
  mkdir,
  readFile,
  stat,
  writeFile,
} from "node:fs/promises";
import { createHash } from "node:crypto";
import { basename, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";

function captureDescriptor(sourcePath) {
  const sourceName = basename(sourcePath);
  const normalized = sourceName.toLowerCase();
  if (normalized === "ai-manager.exe") {
    return {
      captureRole: "tauri-patched-application",
      capturedName: "packaged-ai-manager.exe",
    };
  }
  if (normalized.endsWith("-setup.exe")) {
    return {
      captureRole: "unsigned-nsis-installer",
      capturedName: "unsigned-nsis-installer.exe",
    };
  }
  if (normalized.endsWith(".dll")) {
    const safeName = normalized.replace(/[^a-z0-9._-]+/g, "-");
    return {
      captureRole: "unsigned-nsis-plugin",
      capturedName: `unsigned-nsis-plugin-${safeName}`,
    };
  }
  if (/^makensis[a-z0-9_-]+$/i.test(sourceName)) {
    return {
      captureRole: "unsigned-nsis-uninstaller-stub",
      capturedName: "unsigned-nsis-uninstaller-stub.exe",
    };
  }
  throw new Error(`unexpected Windows QA signing input: ${sourceName}`);
}

export async function captureWindowsQaSigningInput(
  sourcePath,
  captureDirectory,
) {
  if (!sourcePath || !captureDirectory) {
    throw new Error("source path and capture directory are required");
  }

  const source = resolve(sourcePath);
  const captureRoot = resolve(captureDirectory);
  const sourceInfo = await stat(source);
  if (!sourceInfo.isFile())
    throw new Error("Windows QA signing input must be a file");

  const bytes = await readFile(source);
  if (bytes.length < 2 || bytes[0] !== 0x4d || bytes[1] !== 0x5a) {
    throw new Error("Windows QA signing input is not a PE executable");
  }

  const { captureRole, capturedName } = captureDescriptor(source);
  const destination = join(captureRoot, capturedName);
  const evidencePath = `${destination}.json`;
  const evidence = {
    captureRole,
    sourceName: basename(source),
    capturedName,
    size: bytes.length,
    sha256: createHash("sha256").update(bytes).digest("hex"),
    authenticodeState: "not-verified-qa-capture",
  };

  await mkdir(captureRoot, { recursive: true, mode: 0o700 });
  await copyFile(source, destination, constants.COPYFILE_EXCL);
  await writeFile(evidencePath, `${JSON.stringify(evidence, null, 2)}\n`, {
    encoding: "utf8",
    flag: "wx",
    mode: 0o600,
  });

  return { destination, evidencePath, evidence };
}

async function main() {
  const sourcePath = process.argv[2];
  const captureDirectory = process.env.AI_MANAGER_WINDOWS_QA_CAPTURE_DIR;
  if (!sourcePath || !captureDirectory) {
    throw new Error(
      "usage: AI_MANAGER_WINDOWS_QA_CAPTURE_DIR=<dir> node scripts/capture-windows-qa-signing-input.mjs <artifact>",
    );
  }

  const result = await captureWindowsQaSigningInput(
    sourcePath,
    captureDirectory,
  );
  process.stdout.write(
    `${JSON.stringify({ ...result.evidence, destination: result.destination })}\n`,
  );
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
