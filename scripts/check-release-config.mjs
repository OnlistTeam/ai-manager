import { createHash } from "node:crypto";
import { readFile } from "node:fs/promises";
import { resolve } from "node:path";
import {
  PHASE7_TRANSITIONAL_RULES,
  validateReleaseContract,
} from "./release-contract.mjs";

const root = process.cwd();
const strict = process.argv.includes("--strict");

async function text(path) {
  return readFile(resolve(root, path), "utf8");
}

async function json(path) {
  return JSON.parse(await text(path));
}

async function sha256(path) {
  const contents = await readFile(resolve(root, path));
  return createHash("sha256").update(contents).digest("hex");
}

const violations = validateReleaseContract({
  packageJson: await json("package.json"),
  tauriConfig: await json("src-tauri/tauri.conf.json"),
  cargoToml: await text("src-tauri/Cargo.toml"),
  workflow: await text(".github/workflows/release.yml"),
  infoPlist: await text("src-tauri/Info.plist"),
  assetDigests: {
    appIcon: await sha256("src/assets/icons/app-icon.png"),
    bundleIcon: await sha256("src-tauri/icons/icon.png"),
    dmgBackground: await sha256("src-tauri/icons/dmg-background.png"),
    dmgBackground2x: await sha256("src-tauri/icons/dmg-background@2x.png"),
    trayIcon: await sha256(
      "src-tauri/icons/tray/macos/statusbar_template_3x.png",
    ),
  },
});

const unexpected = violations.filter(
  (violation) => !PHASE7_TRANSITIONAL_RULES.has(violation.id),
);

if (unexpected.length > 0 || (strict && violations.length > 0)) {
  console.error("Release contract violations:");
  for (const violation of strict ? violations : unexpected) {
    console.error("- " + violation.id + ": " + violation.message);
  }
  process.exitCode = 1;
} else if (violations.length > 0) {
  console.warn(
    "Release contract has " +
      violations.length +
      " audited Phase 7 transition item(s).",
  );
  for (const violation of violations) {
    console.warn("- transitional " + violation.id);
  }
} else {
  console.log("Release contract OK");
}
