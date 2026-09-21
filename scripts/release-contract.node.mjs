import assert from "node:assert/strict";
import test from "node:test";
import { validateReleaseContract } from "./release-contract.mjs";

const PRODUCT_UPDATE_ENDPOINT =
  "https://dl.aimanager.tools/ai-manager/latest.json";

function fixture() {
  return {
    packageJson: { name: "ai-manager", version: "0.1.0" },
    tauriConfig: {
      productName: "AI Manager",
      version: "0.1.0",
      identifier: "tools.aimanager.desktop",
      bundle: {
        windows: {
          wix: { template: "wix/per-user-main.wxs" },
        },
      },
      plugins: {
        updater: {
          pubkey: "product-public-key",
          endpoints: [PRODUCT_UPDATE_ENDPOINT],
        },
      },
    },
    cargoToml: [
      "[package]",
      'name = "ai-manager"',
      'version = "0.1.0"',
      'description = "AI Manager desktop application"',
      'repository = "https://github.com/OnlistTeam/ai-manager"',
      "",
      "[lib]",
      'name = "ai_manager_lib"',
    ].join("\n"),
    workflow: [
      "env:",
      '  AI_MANAGER_UPDATE_CHANNEL_ENABLED: "1"',
      "  AI_MANAGER_UPDATE_CHANNEL: ${{ inputs.prerelease && 'staging' || 'stable' }}",
      "jobs:",
      "  release:",
      "    runs-on: windows-2022",
      "    steps:",
      "      - run: $digest = (Get-FileHash -Path $destination -Algorithm SHA256).Hash",
      '      - run: "$digest  $name" | Set-Content -Path "$destination.sha256"',
      "      - run: echo aarch64-apple-darwin x86_64-apple-darwin x86_64-unknown-linux-gnu",
      "      - run: echo --bundles appimage,deb AI-Manager-$version-Linux-x64.AppImage AI-Manager-$version-Linux-x64.deb",
      "      - run: echo darwin-aarch64 darwin-x86_64 windows-x86_64 linux-x86_64",
      "      - run: echo prerelease publication requires a prerelease SemVer tag",
      "      - run: echo stable publication requires a stable SemVer tag",
      "  publish-release:",
      "    environment: release",
      "    steps:",
      "      - run: echo AI Manager",
    ].join("\n"),
    infoPlist: "<plist><dict></dict></plist>",
    assetDigests: {
      appIcon: "product-app-icon",
      bundleIcon: "product-bundle-icon",
      dmgBackground: "product-dmg",
      dmgBackground2x: "product-dmg-2x",
      trayIcon: "product-tray-icon",
    },
  };
}

function ids(input) {
  return validateReleaseContract(input).map((violation) => violation.id);
}

test("accepts an unsigned, checksummed four-target release contract", () => {
  assert.deepEqual(validateReleaseContract(fixture()), []);
});

test("rejects a Tauri-level return of Windows platform signing", () => {
  for (const windows of [
    { certificateThumbprint: "ABCDEF0123456789" },
    { signCommand: "artifact-signing-cli -d AIManager %1" },
  ]) {
    const input = fixture();
    input.tauriConfig.bundle.windows = windows;
    assert(
      ids(input).includes("workflow.windows-authenticode"),
      JSON.stringify(windows),
    );
  }
});

test("rejects every inherited product identity surface", () => {
  const input = fixture();
  input.packageJson.name = "cc-switch";
  input.tauriConfig.productName = "CC Switch";
  input.tauriConfig.identifier = "com.ccswitch.desktop";
  input.tauriConfig.plugins["deep-link"] = {
    desktop: { schemes: ["ccswitch"] },
  };
  input.infoPlist =
    "<string>CC Switch Deep Link</string><string>ccswitch</string>";
  input.cargoToml = input.cargoToml
    .replace('name = "ai-manager"', 'name = "cc-switch"')
    .replace(
      'description = "AI Manager desktop application"',
      'description = "All-in-One Assistant"',
    )
    .replace(
      "https://github.com/OnlistTeam/ai-manager",
      "https://github.com/farion1231/cc-switch",
    );

  assert.deepEqual(
    new Set(ids(input)),
    new Set([
      "identity.package-name",
      "identity.product-name",
      "identity.bundle-id",
      "identity.cargo-name",
      "identity.cargo-description",
      "identity.cargo-repository",
      "identity.deep-link",
    ]),
  );
});

test("rejects version drift", () => {
  const input = fixture();
  input.tauriConfig.version = "0.1.1";
  assert(ids(input).includes("version.mismatch"));
});

test("rejects upstream updater trust", () => {
  const input = fixture();
  input.tauriConfig.plugins.updater.pubkey =
    "dW50cnVzdGVkIGNvbW1lbnQ6IG1pbmlzaWduIHB1YmxpYyBrZXk6IEM4MDI4QzlBNTczOTI4RTMKUldUaktEbFhtb3dDeUM5US9kT0FmdGR5Ti9vQzcwa2dTMlpibDVDUmQ2M0VGTzVOWnd0SGpFVlEK";
  input.tauriConfig.plugins.updater.endpoints = [
    "https://dl.ccswitch.io/latest.json",
  ];
  assert(ids(input).includes("updater.public-key"));
  assert(ids(input).includes("updater.endpoint"));
});

test("rejects an empty, private GitHub, or lookalike updater channel", () => {
  const input = fixture();
  input.tauriConfig.plugins.updater.endpoints = [];
  assert(ids(input).includes("updater.endpoint"));

  input.tauriConfig.plugins.updater.endpoints = [
    "https://github.com/OnlistTeam/ai-manager/releases/latest/download/latest.json",
  ];
  assert(ids(input).includes("updater.endpoint"));

  for (const endpoint of [
    "https://dl.aimanager.tools.example/ai-manager/latest.json",
    "https://dl.aimanager.tools/ai-manager/staging/latest.json",
    "http://dl.aimanager.tools/ai-manager/latest.json",
    "https://user:pass@dl.aimanager.tools/ai-manager/latest.json",
    "https://dl.aimanager.tools:8443/ai-manager/latest.json",
    "https://dl.aimanager.tools/ai-manager/latest.json?channel=stable",
    "https://dl.aimanager.tools/ai-manager/latest.json#stable",
  ]) {
    input.tauriConfig.plugins.updater.endpoints = [endpoint];
    assert(ids(input).includes("updater.endpoint"), endpoint);
  }
});

test("rejects a second endpoint or a release channel that can drift from the tag", () => {
  const input = fixture();
  input.tauriConfig.plugins.updater.endpoints = [
    PRODUCT_UPDATE_ENDPOINT,
    PRODUCT_UPDATE_ENDPOINT,
  ];
  input.workflow = input.workflow.replace(
    "  AI_MANAGER_UPDATE_CHANNEL: ${{ inputs.prerelease && 'staging' || 'stable' }}\n",
    "",
  );

  assert(ids(input).includes("updater.endpoint"));
  assert(ids(input).includes("updater.release-channel"));
});

test("rejects a workflow that reintroduces Windows platform signing", () => {
  for (const step of [
    "      - run: Resolve-Path scripts/sign-windows.ps1",
    "      - run: echo signCommand",
    '      - run: & "$env:SIGNTOOL_PATH" verify /pa /all /v app.msi',
    "      - run: signtool sign /fd SHA256 app.exe",
    "      - run: Get-AuthenticodeSignature -FilePath app.msi",
    "      - run: artifact-signing-cli -d AIManager app.exe",
    "      - uses: azure/artifact-signing-action@v1",
    "      - run: Install-Module -Name ArtifactSigning -RequiredVersion 0.1.8",
    "        env:\n          WINDOWS_CERTIFICATE: ${{ secrets.WINDOWS_CERTIFICATE }}",
    "        env:\n          WINDOWS_SIGNING_PROVIDER: ${{ vars.WINDOWS_SIGNING_PROVIDER }}",
    "        env:\n          WINDOWS_ARTIFACT_SIGNING_PROFILE: ${{ vars.WINDOWS_ARTIFACT_SIGNING_PROFILE }}",
    "        env:\n          AZURE_CLIENT_ID: ${{ secrets.AZURE_CLIENT_ID }}",
  ]) {
    const input = fixture();
    input.workflow += "\n" + step + "\n";
    assert(ids(input).includes("workflow.windows-authenticode"), step);
  }
});

test("rejects a Windows release that drops its SHA-256 record", () => {
  const input = fixture();
  input.workflow = input.workflow.replace(
    "      - run: $digest = (Get-FileHash -Path $destination -Algorithm SHA256).Hash\n",
    "",
  );

  assert(ids(input).includes("workflow.windows-checksum"));
});

test("rejects the Windows QA capture command in a release workflow", () => {
  const input = fixture();
  input.workflow +=
    "\n      - run: node scripts/capture-windows-qa-signing-input.mjs app.exe\n";

  assert(ids(input).includes("workflow.windows-qa-capture"));
});

test("rejects target creep, brand residue, and secret fragments", () => {
  const input = fixture();
  input.workflow += [
    "",
    "      - run: echo CC-Switch",
    "      - run: echo universal-apple-darwin aarch64-unknown-linux-gnu windows-11-arm --bundles rpm",
    '      - run: echo "$RAW" | head -c 10',
  ].join("\n");

  assert(ids(input).includes("workflow.brand"));
  assert(ids(input).includes("workflow.target-scope"));
  assert(ids(input).includes("workflow.secret-fragment"));
});

test("rejects publication outside the protected release environment", () => {
  const input = fixture();
  input.workflow = input.workflow.replace("    environment: release\n", "");
  assert(ids(input).includes("workflow.publish-environment"));
});

test("rejects incomplete latest.json platform generation", () => {
  const input = fixture();
  input.workflow = input.workflow.replace("darwin-x86_64", "darwin-intel");
  assert(ids(input).includes("workflow.latest-platforms"));
});

test("rejects a Linux release missing the approved installer pair", () => {
  const input = fixture();
  input.workflow = input.workflow.replace(
    "--bundles appimage,deb",
    "--bundles appimage",
  );
  assert(ids(input).includes("workflow.linux-release"));
});

test("rejects byte-identical upstream artwork", () => {
  const input = fixture();
  input.assetDigests.dmgBackground2x =
    "04225b1b9c54569ec1ec850ad9f1c9f33ca4f286dab001a3392c0460deb342e5";
  assert(ids(input).includes("asset.upstream-brand"));
});
