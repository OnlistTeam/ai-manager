import assert from "node:assert/strict";
import { readdir, readFile } from "node:fs/promises";
import test from "node:test";

const root = new URL("../", import.meta.url);

async function source(path) {
  return readFile(new URL(path, root), "utf8");
}

function workflowJob(workflow, name) {
  const lines = workflow.split(/\r?\n/);
  const start = lines.findIndex((line) => line === `  ${name}:`);
  assert(start >= 0, `missing workflow job: ${name}`);

  let end = lines.length;
  for (let index = start + 1; index < lines.length; index += 1) {
    if (/^ {2}[A-Za-z0-9_-]+:\s*$/.test(lines[index])) {
      end = index;
      break;
    }
  }
  return lines.slice(start, end).join("\n");
}

test("release publication is manual, strict, and protected", async () => {
  const workflow = await source(".github/workflows/release.yml");
  const preflight = workflowJob(workflow, "preflight");
  const publish = workflowJob(workflow, "publish-release");
  const distribution = workflowJob(workflow, "sync-distribution");

  assert.match(workflow, /^on:\n\s+workflow_dispatch:/m);
  assert.doesNotMatch(workflow, /^\s+push:\s*$|^\s+tags:\s*$/m);
  assert.match(preflight, /pnpm check:release -- --strict/);
  assert.match(publish, /^\s{4}environment:\s*release\s*$/m);
  assert.match(publish, /needs:\s*assemble-release/);
  assert.match(distribution, /needs:\s*publish-release/);
  assert.match(distribution, /uses:\s*\.\/\.github\/workflows\/sync-r2\.yml/);
  assert.match(workflow, /prerelease:[\s\S]*default:\s*true/);
  assert.match(
    workflow,
    /AI_MANAGER_UPDATE_CHANNEL:\s*\$\{\{ inputs\.prerelease && 'staging' \|\| 'stable' \}\}/,
  );
  assert.match(
    preflight,
    /prerelease publication requires a prerelease SemVer tag/,
  );
  assert.match(preflight, /stable publication requires a stable SemVer tag/);
});

test("build matrix contains exactly the four release targets", async () => {
  const workflow = await source(".github/workflows/release.yml");
  const macos = workflowJob(workflow, "build-macos");
  const windows = workflowJob(workflow, "build-windows");
  const linux = workflowJob(workflow, "build-linux");

  assert.match(macos, /aarch64-apple-darwin/);
  assert.match(macos, /x86_64-apple-darwin/);
  assert.match(macos, /macos-15-intel/);
  assert.match(windows, /x86_64-pc-windows-msvc/);
  assert.match(linux, /runs-on:\s*ubuntu-22\.04/);
  assert.match(linux, /x86_64-unknown-linux-gnu/);
  assert.match(linux, /--bundles appimage,deb/);
  assert.match(linux, /Linux-x64\.AppImage/);
  assert.match(linux, /Linux-x64\.deb/);
  assert.doesNotMatch(
    workflow,
    /universal-apple-darwin|aarch64-pc-windows-msvc|windows-11-arm|aarch64-unknown-linux-gnu|--bundles\s+[^\n]*\brpm\b/i,
  );
});

test("Linux updater is the verified AppImage and both installers are checksummed", async () => {
  const workflow = await source(".github/workflows/release.yml");
  const linux = workflowJob(workflow, "build-linux");

  assert.match(linux, /libwebkit2gtk-4\.1-dev/);
  assert.match(linux, /patchelf/);
  assert.match(linux, /test -x "\$appimage"/);
  assert.match(linux, /dpkg-deb --field "\$deb" Package/);
  assert.match(linux, /dpkg-deb --field "\$deb" Architecture/);
  // The bundler signs the AppImage in place, so the installer and the update
  // payload are one file. Proving the signature accompanies it is what is left;
  // an archive step here would mean the job is unwrapping something that no
  // longer exists.
  assert.match(linux, /test -s "\$appimage\.sig"/);
  assert.match(
    linux,
    /cp "\$appimage\.sig" "release-assets\/\$appimage_name\.sig"/,
  );
  assert.doesNotMatch(linux, /tar -[tx]zf/);
  assert.doesNotMatch(linux, /AppImage\.tar\.gz/);
  assert.match(linux, /sha256sum "release-assets\/\$name"/);
  assert.doesNotMatch(linux, /sh\s+-c|bash\s+-c/);

  const structuralVerify = linux.indexOf("Verify Linux package structure");
  const prepare = linux.indexOf("Prepare verified Linux release files");
  const upload = linux.indexOf("actions/upload-artifact@v7");
  assert(
    structuralVerify >= 0 && prepare > structuralVerify && upload > prepare,
  );
});

test("macOS artifacts are signed, notarized, stapled, and verified before upload", async () => {
  const workflow = await source(".github/workflows/release.yml");
  const macos = workflowJob(workflow, "build-macos");
  const notarizer = await source("scripts/notarize-macos.sh");

  assert.match(macos, /security create-keychain/);
  assert.match(macos, /Developer ID Application/);
  assert.match(macos, /APPLE_TEAM_ID/);
  assert.match(macos, /codesign --verify --deep --strict/);
  assert.match(macos, /spctl -a -t exec/);
  assert.match(macos, /xcrun stapler staple/);
  assert.match(macos, /xcrun stapler validate/);
  assert.match(macos, /if:\s*always\(\)/);
  assert.match(macos, /security delete-keychain/);

  assert.match(notarizer, /notarytool submit/);
  assert.match(notarizer, /--no-wait/);
  assert.match(notarizer, /notarytool wait/);
  assert.match(notarizer, /for attempt in 1 2 3/);
  assert.match(notarizer, /notarytool log/);
  assert.match(notarizer, /Accepted/);

  const verify = macos.indexOf("codesign --verify --deep --strict");
  const upload = macos.indexOf("actions/upload-artifact@v7");
  assert(verify >= 0 && upload > verify);
});

test("macOS DMG creation is headless, styled, dependency-locked, and signed", async () => {
  const workflow = await source(".github/workflows/release.yml");
  const macos = workflowJob(workflow, "build-macos");
  const [settings, requirements] = await Promise.all([
    source("scripts/dmgbuild-settings.py"),
    source("scripts/requirements-dmgbuild.txt"),
  ]);

  assert.doesNotMatch(macos, /create-dmg|osascript/i);
  assert.match(macos, /--require-hashes/);
  assert.match(macos, /--only-binary=:all:/);
  assert.match(macos, /dmgbuild-settings\.py/);
  assert.match(macos, /src-tauri\/icons\/dmg-background\.png/);
  assert.match(macos, /src-tauri\/icons\/icon\.icns/);

  assert.match(settings, /application\s*=\s*str\(application_path\)/);
  assert.match(settings, /files\s*=\s*\[application\]/);
  assert.match(settings, /symlinks\s*=\s*\{["']Applications["']/);
  assert.match(settings, /hide_extensions\s*=\s*\[\s*\]/);
  assert.match(settings, /background\s*=\s*str\(background_path\)/);
  assert.match(
    settings,
    /window_rect\s*=\s*\(\(200,\s*120\),\s*\(660,\s*400\)\)/,
  );
  assert.match(settings, /app_name:\s*\(180,\s*220\)/);
  assert.match(settings, /["']Applications["']:\s*\(480,\s*220\)/);

  for (const lockedPackage of [
    "dmgbuild==1.6.7",
    "ds_store==1.3.3",
    "mac_alias==2.2.3",
  ]) {
    assert.match(requirements, new RegExp(`^${lockedPackage}`, "m"));
  }
  assert.equal(
    [...requirements.matchAll(/--hash=sha256:[a-f0-9]{64}/g)].length,
    3,
  );

  const buildDmg = macos.indexOf("dmgbuild-settings.py");
  const signDmg = macos.indexOf(
    'codesign --force --timestamp --sign "$APPLE_SIGNING_IDENTITY"',
  );
  const notarizeDmg = macos.indexOf("Notarize DMG");
  assert(buildDmg >= 0 && signDmg > buildDmg && notarizeDmg > signDmg);
});

test("Windows publishes an unsigned, marker-verified, checksummed MSI", async () => {
  const workflow = await source(".github/workflows/release.yml");
  const windows = workflowJob(workflow, "build-windows");
  const markerVerifier = await source(
    "scripts/verify-windows-bundle-marker.mjs",
  );

  // Windows ships without a platform code signature by owner decision
  // (2026-09-20). The updater minisign signature is a separate trust chain and
  // must stay.
  assert.doesNotMatch(windows, /signtool|SIGNTOOL_PATH/i);
  assert.doesNotMatch(windows, /Get-AuthenticodeSignature|Authenticode/i);
  assert.doesNotMatch(windows, /signCommand|sign-windows\.ps1/i);
  assert.doesNotMatch(
    windows,
    /WINDOWS_(?:CERTIFICATE|SIGNER|SIGNING|TIMESTAMP)/,
  );
  assert.doesNotMatch(windows, /WINDOWS_ARTIFACT_SIGNING|ArtifactSigning/i);
  assert.doesNotMatch(
    windows,
    /azure\/login@|azure\/artifact-signing-action@/i,
  );
  assert.doesNotMatch(windows, /AZURE_(?:CLIENT_ID|TENANT_ID|SUBSCRIPTION_ID)/);
  assert.doesNotMatch(windows, /id-token:\s*write/);
  assert.doesNotMatch(windows, /capture-windows-qa-signing-input/);

  assert.match(windows, /TAURI_SIGNING_PRIVATE_KEY/);
  assert.match(windows, /TAURI_SIGNING_PRIVATE_KEY_PASSWORD/);
  assert.match(windows, /Tauri updater signing secrets are required/);
  // msiexec detaches, so the extraction must be waited on explicitly or the
  // step reads an exit code that was never set and passes on an empty result.
  assert.match(windows, /Start-Process -FilePath 'msiexec\.exe' -Wait/);
  assert.match(windows, /verify-windows-bundle-marker\.mjs/);
  assert.match(windows, /pnpm tauri signer sign/);
  assert.match(windows, /Get-FileHash -Path \$destination -Algorithm SHA256/);
  assert.match(windows, /"\$destination\.sha256"/);
  assert.match(markerVerifier, /__TAURI_BUNDLE_TYPE_VAR_UNK/);
  assert.match(markerVerifier, /__TAURI_BUNDLE_TYPE_VAR_MSI/);

  const build = windows.indexOf("--no-bundle");
  const bundle = windows.indexOf("pnpm tauri bundle");
  const updaterSign = windows.indexOf("pnpm tauri signer sign");
  const extract = windows.indexOf("Start-Process -FilePath 'msiexec.exe'");
  const marker = windows.indexOf("verify-windows-bundle-marker.mjs");
  const checksum = windows.indexOf("Get-FileHash");
  const upload = windows.indexOf("actions/upload-artifact@v7");
  assert(build >= 0 && bundle > build);
  assert.doesNotMatch(
    windows.slice(0, bundle),
    /steps\.windows-paths\.outputs\.exe/,
  );
  assert(updaterSign > bundle && extract > updaterSign && marker > extract);
  assert(checksum > marker && upload > checksum);
});

test("Windows bundling pins the byte-marker-capable Tauri CLI", async () => {
  const packageJson = JSON.parse(await source("package.json"));

  const expected = {
    "@tauri-apps/api": "2.10.1",
    "@tauri-apps/plugin-dialog": "2.6.0",
    "@tauri-apps/plugin-process": "2.3.1",
    "@tauri-apps/plugin-updater": "2.10.1",
  };
  for (const [name, version] of Object.entries(expected)) {
    assert.equal(packageJson.dependencies?.[name], version, name);
  }
  assert.equal(
    packageJson.devDependencies?.["@tauri-apps/cli"],
    "2.11.4",
    "@tauri-apps/cli",
  );
});

test("release payload is assembled and verified before protected publication", async () => {
  const workflow = await source(".github/workflows/release.yml");
  const assemble = workflowJob(workflow, "assemble-release");
  const publish = workflowJob(workflow, "publish-release");

  for (const key of [
    "darwin-aarch64",
    "darwin-x86_64",
    "windows-x86_64",
    "linux-x86_64",
  ]) {
    assert.match(assemble, new RegExp(key));
  }
  assert.match(assemble, /minisign -Vm/);
  assert.match(assemble, /assemble-latest-json\.mjs/);
  assert.match(assemble, /actions\/upload-artifact@v7/);
  assert.match(publish, /softprops\/action-gh-release@[0-9a-f]{40} # v3/);
  assert.match(publish, /AI Manager/);
  assert.match(publish, /AI Manager desktop release\./);
  assert.match(publish, /Windows artifacts carry no platform code signature/);
  assert.match(publish, /unknown publisher/);
  assert.match(publish, /Windows-x64\.msi\.sha256/);
  assert.doesNotMatch(publish, /AI Manager desktop prerelease\./);
  assert.doesNotMatch(workflow, /CC[- ]Switch|ccswitch\.io/i);
  assert.doesNotMatch(
    workflow,
    /\bRAW\b[\s\S]{0,120}head\s+-c|head\s+-c[\s\S]{0,120}\bRAW\b/i,
  );
});

test("third-party actions are pinned to a commit, not a movable tag", async () => {
  const directory = new URL(".github/workflows/", root);
  const files = (await readdir(directory)).filter((name) =>
    name.endsWith(".yml"),
  );
  assert(files.length > 0, "no workflows found");

  const unpinned = [];
  for (const file of files) {
    const workflow = await readFile(new URL(file, directory), "utf8");
    for (const [, reference] of workflow.matchAll(/^\s*uses:\s*(\S+)/gm)) {
      // Local reusable workflows carry no supply-chain risk, and GitHub owns
      // the `actions` organisation itself: a moving tag there is as trusted as
      // the runner. Everything else runs someone else's code with our token,
      // so the exact commit has to be named.
      if (reference.startsWith("./") || reference.startsWith("actions/")) {
        continue;
      }
      if (!/@[0-9a-f]{40}$/.test(reference)) {
        unpinned.push(`${file}: ${reference}`);
      }
    }
  }
  assert.deepEqual(unpinned, []);
});
