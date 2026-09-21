const UPSTREAM_UPDATER_KEY =
  "dW50cnVzdGVkIGNvbW1lbnQ6IG1pbmlzaWduIHB1YmxpYyBrZXk6IEM4MDI4QzlBNTczOTI4RTMKUldUaktEbFhtb3dDeUM5US9kT0FmdGR5Ti9vQzcwa2dTMlpibDVDUmQ2M0VGTzVOWnd0SGpFVlEK";

const UPSTREAM_ASSET_DIGESTS = new Set([
  "04225b1b9c54569ec1ec850ad9f1c9f33ca4f286dab001a3392c0460deb342e5",
  "f18645510570b45df07f8b785ff7ee979f4c7c1dd828276053875292313e3ec0",
  "8c167919ba52ed97aaa5b612819821a0c6dba2c7cd2c03a09df0a2086451d2aa",
  "65c365e7c32929807deec2d7469eed2d5c06f21ce86854a4848057d2df588ab8",
  "737004c91a766a67e57a62f695a28e2390a6fbd48efcb1d3eda1fe5bf289c3f4",
  "c82ff0ffb2800fbcbbdd88b92d1844527ef5fb40eb5d6fb3213bab505b2ee007",
]);

const PRODUCT_UPDATE_HOST = "dl.aimanager.tools";
const PRODUCT_UPDATE_PATH = "/ai-manager/latest.json";

export const PHASE7_TRANSITIONAL_RULES = new Set();

function isProductUpdateEndpoint(value) {
  if (typeof value !== "string") return false;

  let parsed;
  try {
    parsed = new URL(value);
  } catch {
    return false;
  }

  return (
    parsed.protocol === "https:" &&
    !parsed.username &&
    !parsed.password &&
    !parsed.port &&
    !parsed.search &&
    !parsed.hash &&
    parsed.hostname === PRODUCT_UPDATE_HOST &&
    parsed.pathname === PRODUCT_UPDATE_PATH
  );
}

function add(violations, id, message) {
  violations.push({ id, message });
}

function packageSection(cargoToml) {
  const match = cargoToml.match(
    /^\[package\]\s*\n([\s\S]*?)(?=^\[[^\]]+\]\s*$)/m,
  );
  return match?.[1] ?? "";
}

function cargoString(section, key) {
  const escaped = key.replace(/[-/\\^$*+?.()|[\]{}]/g, "\\$&");
  const expression = new RegExp("^" + escaped + '\\s*=\\s*"([^"]+)"', "m");
  return section.match(expression)?.[1] ?? "";
}

function workflowJob(workflow, name) {
  const lines = workflow.split(/\r?\n/);
  const start = lines.findIndex((line) => line === "  " + name + ":");
  if (start < 0) return "";

  let end = lines.length;
  for (let index = start + 1; index < lines.length; index += 1) {
    if (/^ {2}[A-Za-z0-9_-]+:\s*$/.test(lines[index])) {
      end = index;
      break;
    }
  }
  return lines.slice(start, end).join("\n");
}

// Windows ships without a platform code signature by owner decision
// (2026-09-20): the fork matches upstream CC Switch, which also publishes an
// unsigned Windows installer. This detects a silent return of Authenticode
// configuration, which would reinstate an unfunded certificate prerequisite
// and fail every Windows release job.
function findWindowsAuthenticodeConfiguration(tauriConfig, workflow) {
  const windows = tauriConfig?.bundle?.windows ?? {};
  if (windows.certificateThumbprint) {
    return "tauri.conf.json certificateThumbprint";
  }
  if (windows.signCommand) {
    return "tauri.conf.json signCommand";
  }
  if (/\bsignCommand\b/i.test(workflow)) {
    return "workflow signCommand";
  }
  if (/sign-windows\.ps1\b/i.test(workflow)) {
    return "workflow Windows signing script";
  }
  if (/\bsigntool(?:\.exe)?\b|\bSIGNTOOL_PATH\b/i.test(workflow)) {
    return "workflow signtool invocation";
  }
  if (/\bGet-AuthenticodeSignature\b/i.test(workflow)) {
    return "workflow Authenticode verification";
  }
  if (
    /\bartifact-signing-cli\b|azure\/artifact-signing-action@|\bArtifactSigning\b/i.test(
      workflow,
    )
  ) {
    return "workflow Artifact Signing integration";
  }
  if (
    /\bWINDOWS_(?:CERTIFICATE|SIGNER_SUBJECT|SIGNING_PROVIDER|TIMESTAMP_URL|ARTIFACT_SIGNING)[A-Z_]*\b|\bAZURE_(?:CLIENT_ID|TENANT_ID|SUBSCRIPTION_ID)\b/.test(
      workflow,
    )
  ) {
    return "workflow Windows signing credential";
  }
  return "";
}

function publishesWindowsChecksum(workflow) {
  return (
    /Get-FileHash[^\n]*-Algorithm SHA256/i.test(workflow) &&
    /\$destination\.sha256/.test(workflow)
  );
}

export function validateReleaseContract(input) {
  const violations = [];
  const packageJson = input.packageJson;
  const tauriConfig = input.tauriConfig;
  const packageBlock = packageSection(input.cargoToml);
  const cargoName = cargoString(packageBlock, "name");
  const cargoVersion = cargoString(packageBlock, "version");
  const cargoDescription = cargoString(packageBlock, "description");
  const cargoRepository = cargoString(packageBlock, "repository");
  const workflow = input.workflow;
  const infoPlist = input.infoPlist;
  const endpoints = tauriConfig?.plugins?.updater?.endpoints ?? [];
  const schemes = tauriConfig?.plugins?.["deep-link"]?.desktop?.schemes ?? [];

  if (packageJson.name !== "ai-manager") {
    add(
      violations,
      "identity.package-name",
      "package.json name must be ai-manager",
    );
  }
  if (tauriConfig.productName !== "AI Manager") {
    add(
      violations,
      "identity.product-name",
      "Tauri productName must be AI Manager",
    );
  }
  if (tauriConfig.identifier !== "tools.aimanager.desktop") {
    add(
      violations,
      "identity.bundle-id",
      "Tauri identifier must be the approved AI Manager identifier",
    );
  }
  if (cargoName !== "ai-manager") {
    add(
      violations,
      "identity.cargo-name",
      "Cargo package name must be ai-manager",
    );
  }
  if (!/^AI Manager\b/.test(cargoDescription)) {
    add(
      violations,
      "identity.cargo-description",
      "Cargo package description must describe AI Manager",
    );
  }
  if (
    !/github\.com\/OnlistTeam\/ai-manager(?:\.git)?$/i.test(cargoRepository)
  ) {
    add(
      violations,
      "identity.cargo-repository",
      "Cargo repository must point at the approved AI Manager repository",
    );
  }
  if (
    schemes.some((scheme) => scheme.toLowerCase() === "ccswitch") ||
    /<string>ccswitch<\/string>|CC Switch Deep Link/i.test(infoPlist)
  ) {
    add(
      violations,
      "identity.deep-link",
      "the inherited ccswitch deep-link registration must not ship",
    );
  }

  const versions = [packageJson.version, cargoVersion, tauriConfig.version];
  if (!versions[0] || !versions.every((version) => version === versions[0])) {
    add(
      violations,
      "version.mismatch",
      "package, Cargo, and Tauri versions must match exactly",
    );
  }

  if (tauriConfig?.plugins?.updater?.pubkey === UPSTREAM_UPDATER_KEY) {
    add(
      violations,
      "updater.public-key",
      "the updater public key is still the upstream CC Switch key",
    );
  }
  if (endpoints.length !== 1 || !isProductUpdateEndpoint(endpoints[0])) {
    add(
      violations,
      "updater.endpoint",
      "configure exactly one product-owned HTTPS AI Manager updater endpoint",
    );
  }

  if (
    !/AI_MANAGER_UPDATE_CHANNEL_ENABLED:\s*["']1["']/.test(workflow) ||
    !/AI_MANAGER_UPDATE_CHANNEL:\s*\$\{\{\s*inputs\.prerelease\s*&&\s*["']staging["']\s*\|\|\s*["']stable["']\s*\}\}/.test(
      workflow,
    ) ||
    !/prerelease publication requires a prerelease SemVer tag/.test(workflow) ||
    !/stable publication requires a stable SemVer tag/.test(workflow)
  ) {
    add(
      violations,
      "updater.release-channel",
      "release builds must compile an isolated stable or staging updater channel that matches tag semantics",
    );
  }

  if (/CC[- ]Switch|ccswitch\.io/i.test(workflow)) {
    add(
      violations,
      "workflow.brand",
      "release workflow still publishes CC Switch branding",
    );
  }
  if (
    /windows-11-arm|universal-apple-darwin|aarch64-pc-windows-msvc|windows-aarch64|aarch64-unknown-linux-gnu|--bundles\s+[^\n]*\brpm\b/i.test(
      workflow,
    )
  ) {
    add(
      violations,
      "workflow.target-scope",
      "release workflow contains a target outside the approved four-target matrix",
    );
  }
  if (
    !/x86_64-unknown-linux-gnu/.test(workflow) ||
    !/--bundles\s+appimage,deb\b/.test(workflow) ||
    !/Linux-x64\.AppImage\b/.test(workflow) ||
    !/Linux-x64\.deb\b/.test(workflow)
  ) {
    add(
      violations,
      "workflow.linux-release",
      "Linux x64 must publish the exact AppImage and deb pair",
    );
  }
  const authenticode = findWindowsAuthenticodeConfiguration(
    tauriConfig,
    workflow,
  );
  if (authenticode) {
    add(
      violations,
      "workflow.windows-authenticode",
      "Windows ships unsigned, but a platform signing configuration is back: " +
        authenticode,
    );
  }
  if (!publishesWindowsChecksum(workflow)) {
    add(
      violations,
      "workflow.windows-checksum",
      "an unsigned Windows MSI must publish a SHA-256 record as its only integrity evidence",
    );
  }
  if (/capture-windows-qa-signing-input/i.test(workflow)) {
    add(
      violations,
      "workflow.windows-qa-capture",
      "release workflow references a QA-only Windows bundle capture command",
    );
  }
  if (
    /(?:head\s+-c\s+\d+[^\n]*\bRAW\b|\bRAW\b[^\n]*head\s+-c\s+\d+)/i.test(
      workflow,
    )
  ) {
    add(
      violations,
      "workflow.secret-fragment",
      "release workflow can print part of a signing secret",
    );
  }

  const publishJob = workflowJob(workflow, "publish-release");
  if (!/^\s{4}environment:\s*release\s*$/m.test(publishJob)) {
    add(
      violations,
      "workflow.publish-environment",
      "publication job must use the protected release environment",
    );
  }

  for (const key of [
    "darwin-aarch64",
    "darwin-x86_64",
    "windows-x86_64",
    "linux-x86_64",
  ]) {
    if (!workflow.includes(key)) {
      add(
        violations,
        "workflow.latest-platforms",
        "latest.json generation is missing " + key,
      );
      break;
    }
  }

  if (
    Object.values(input.assetDigests).some((digest) =>
      UPSTREAM_ASSET_DIGESTS.has(digest),
    )
  ) {
    add(
      violations,
      "asset.upstream-brand",
      "a shipped AI Manager brand asset is still byte-identical to upstream",
    );
  }

  return violations;
}
