import assert from "node:assert/strict";
import { execFile } from "node:child_process";
import { access, readFile, readdir } from "node:fs/promises";
import test from "node:test";
import { promisify } from "node:util";

const root = new URL("../", import.meta.url);

const ROOT_DOCUMENTS = [
  "README.md",
  "CONTRIBUTING.md",
  "SECURITY.md",
  "SUPPORT.md",
  "CODE_OF_CONDUCT.md",
  "THIRD_PARTY_NOTICES.md",
  "AI_RULES.md",
  "LICENSE",
];

const ENGINEERING_DOCUMENTS = [
  "docs/product/design-spec.md",
  "docs/development/RELEASE_RUNBOOK.md",
  "docs/development/BASELINE.md",
];

const POLICY_DOCUMENTS = [
  "CONTRIBUTING.md",
  "SUPPORT.md",
  "SECURITY.md",
  "CODE_OF_CONDUCT.md",
  ".github/ISSUE_TEMPLATE/bug_report.yml",
  ".github/ISSUE_TEMPLATE/config.yml",
  ".github/ISSUE_TEMPLATE/doc_issue.yml",
  ".github/ISSUE_TEMPLATE/feature_request.yml",
  ".github/ISSUE_TEMPLATE/question.yml",
  ".github/pull_request_template.md",
];

// Inherited upstream material and internal working documents that were removed
// when the repository was prepared for publication. They must not come back
// through a careless upstream cherry-pick or a restored branch.
const REMOVED_PATHS = [
  "README_ZH.md",
  "README_JA.md",
  "README_DE.md",
  "CHANGELOG.md",
  "session-manager.md",
  "deplink.html",
  "docs/UPSTREAM_ARCHIVE.md",
  "docs/user-manual",
  "docs/release-notes",
  "docs/guides",
  "docs/images",
  "docs/superpowers",
  "docs/development/plans",
  "docs/development/RELEASE_CREDENTIAL_INVENTORY.md",
  "docs/development/WINDOWS_SIGNING_DECISION.md",
  "assets/partners",
  "assets/screenshots",
];

async function source(path) {
  return readFile(new URL(path, root), "utf8");
}

async function optionalSource(path) {
  try {
    return await source(path);
  } catch (error) {
    if (error?.code === "ENOENT") return null;
    throw error;
  }
}

async function exists(path) {
  try {
    await access(new URL(path, root));
    return true;
  } catch (error) {
    if (error?.code === "ENOENT") return false;
    throw error;
  }
}

async function listFiles(directory) {
  const files = [];
  const entries = await readdir(new URL(directory, root), {
    withFileTypes: true,
  });
  for (const entry of entries) {
    const path = `${directory}${entry.name}`;
    if (entry.isDirectory()) {
      files.push(...(await listFiles(`${path}/`)));
    } else {
      files.push(path);
    }
  }
  return files;
}

test("root documentation describes AI Manager and credits upstream accurately", async () => {
  const readme = await source("README.md");
  assert.match(readme, /^# AI Manager/m);
  assert.match(readme, /docs\/ARCHITECTURE\.md/);
  assert.match(readme, /docs\/product\/design-spec\.md/);
  assert.match(readme, /CONTRIBUTING\.md/);
  assert.match(readme, /THIRD_PARTY_NOTICES\.md/);
  assert.match(readme, /RELEASE_RUNBOOK\.md/);
  assert.match(readme, /github\.com\/farion1231\/cc-switch/);
  assert.match(readme, /AGPL-3\.0-or-later/);
  assert.match(readme, /MIT/);
  assert.doesNotMatch(readme, /ccswitch\.io|farion1231\/cc-switch\/releases/i);
  assert.doesNotMatch(readme, /Sponsor|Advanced Mode/);

  const notices = await source("THIRD_PARTY_NOTICES.md");
  assert.match(notices, /https:\/\/github\.com\/farion1231\/cc-switch/);
  assert.match(notices, /Copyright \(c\) 2025 Jason Young/);
  assert.match(notices, /MIT License/);
  assert.match(notices, /GNU Affero General Public\s+License/);

  // The product is AGPL as a whole; the upstream MIT grant is preserved
  // verbatim in a second file, never folded into or replaced by the AGPL text.
  const license = await source("LICENSE");
  assert.match(license, /^ *GNU AFFERO GENERAL PUBLIC LICENSE$/m);
  assert.match(license, /Version 3, 19 November 2007/);
  assert.doesNotMatch(license, /^MIT License$/m);

  const upstreamLicense = await source("licenses/LICENSE-MIT");
  assert.match(upstreamLicense, /^MIT License$/m);
  assert.match(upstreamLicense, /^Copyright \(c\) 2025 Jason Young$/m);

  for (const path of ["AI_RULES.md"]) {
    const guide = await source(path);
    assert.match(guide, /docs\/product\/design-spec\.md/, `${path} spec link`);
    assert.doesNotMatch(
      guide,
      /https?_proxy=|127\.0\.0\.1:7890/,
      `${path} still carries a personal proxy recipe`,
    );
  }
});

test("documentation set is English and free of private details", async () => {
  const paths = [
    ...ROOT_DOCUMENTS,
    ...ENGINEERING_DOCUMENTS,
    ...(await listFiles(".github/")),
    ...(await listFiles("docs/adr/")).filter((path) => path.endsWith(".md")),
  ];
  for (const path of paths) {
    const contents = await source(path);
    assert.doesNotMatch(
      contents,
      /\p{Script=Han}/u,
      `${path} contains Chinese text`,
    );
    assert.doesNotMatch(
      contents,
      /\/Users\/[a-z]|\/home\/[a-z]+\/|@gmail\.com|127\.0\.0\.1:7890/,
      `${path} contains a local path, personal address, or personal proxy`,
    );
    assert.doesNotMatch(
      contents,
      /-----BEGIN (?:RSA |EC |OPENSSH )?PRIVATE KEY-----/,
      `${path} contains key material`,
    );
  }
});

test("inherited upstream archive and internal working documents stay removed", async () => {
  for (const path of REMOVED_PATHS) {
    assert.equal(
      await exists(path),
      false,
      `${path} was removed for publication and must not return`,
    );
  }
});

test("release runbook documents every workflow-owned configuration name", async () => {
  const workflows = await Promise.all([
    source(".github/workflows/release.yml"),
    source(".github/workflows/sync-r2.yml"),
  ]);
  const workflow = workflows.join("\n");
  const runbook = await source("docs/development/RELEASE_RUNBOOK.md");
  const secrets = new Set(
    [...workflow.matchAll(/secrets\.([A-Z0-9_]+)/g)]
      .map((match) => match[1])
      .filter((name) => name !== "GITHUB_TOKEN"),
  );
  const variables = new Set(
    [...workflow.matchAll(/vars\.([A-Z0-9_]+)/g)].map((match) => match[1]),
  );

  for (const name of [...secrets, ...variables]) {
    assert.match(runbook, new RegExp(`\\b${name}\\b`), `missing ${name}`);
  }
  assert.match(runbook, /^> Status:/m);
  assert.match(
    runbook,
    /published macOS arm64 and x64 artifacts passed Developer ID signing/,
  );
  assert.match(
    runbook,
    /Native Intel and Linux clean-machine\s+validation\s+are still required/,
  );
  assert.match(runbook, /Never describe a local test bundle as official/);
  assert.match(runbook, /Do not rerun an existing release tag/);
  assert.match(runbook, /administratively extracts the MSI/);
  assert.match(runbook, /packaged `ai-manager\.exe`/);
  assert.match(runbook, /windows-installer-qa\.yml/);
  assert.match(runbook, /PublicTrust=false/);
  assert.match(runbook, /uploads audit logs\s+only/i);
  assert.match(runbook, /proves native MSI mechanics only/i);
});

test("Windows release documentation states the unsigned decision and its cost", async () => {
  const workflow = await source(".github/workflows/release.yml");
  const runbook = await source("docs/development/RELEASE_RUNBOOK.md");

  assert.match(
    runbook,
    /Windows ships unsigned by owner decision \(2026-09-20\)/,
  );
  assert.match(runbook, /SmartScreen "unknown publisher" prompt/);
  assert.match(runbook, /SHA-256 is the only\s+integrity evidence/);
  assert.match(runbook, /### 5\.4 Windows platform signing\n\nNone\./);
  assert.match(runbook, /Reversing this needs an owner decision/);

  // The decision is only real if the pipeline cannot quietly reacquire a
  // certificate dependency that no one has funded or approved.
  for (const pattern of [
    /WINDOWS_SIGNING_PROVIDER/,
    /WINDOWS_CERTIFICATE/,
    /WINDOWS_ARTIFACT_SIGNING/,
    /AZURE_(?:CLIENT_ID|TENANT_ID|SUBSCRIPTION_ID)/,
    /signtool/i,
    /Get-AuthenticodeSignature/i,
    /sign-windows\.ps1/i,
  ]) {
    assert.doesNotMatch(workflow, pattern, String(pattern));
    assert.doesNotMatch(runbook, pattern, String(pattern));
  }

  // Procurement packets, credential custody, and unprovisioned vendor
  // credential names live outside the repository.
  assert.doesNotMatch(
    runbook,
    /RELEASE_CREDENTIAL_INVENTORY\.md|WINDOWS_SIGNING_DECISION\.md/,
  );
  assert.doesNotMatch(runbook, /\bSM_[A-Z_]+\b|\bSSLCOM_[A-Z_]+\b/);
  assert.doesNotMatch(runbook, /inquiry packet|quote/i);
  assert.doesNotMatch(workflow, /digicert-keylocker/i);
});

test("release artifact and platform documentation stays exact", async () => {
  const runbook = await source("docs/development/RELEASE_RUNBOOK.md");
  const expectedArtifacts = [
    "macOS-arm64.dmg",
    "macOS-arm64.dmg.sha256",
    "macOS-arm64.app.tar.gz",
    "macOS-arm64.app.tar.gz.sig",
    "macOS-x64.dmg",
    "macOS-x64.dmg.sha256",
    "macOS-x64.app.tar.gz",
    "macOS-x64.app.tar.gz.sig",
    "Windows-x64.msi",
    "Windows-x64.msi.sha256",
    "Windows-x64.msi.sig",
    "Linux-x64.AppImage",
    "Linux-x64.AppImage.sha256",
    "Linux-x64.deb",
    "Linux-x64.deb.sha256",
    "Linux-x64.AppImage.sig",
    "latest.json",
  ];
  for (const suffix of expectedArtifacts) {
    assert.match(runbook, new RegExp(suffix.replaceAll(".", "\\.")));
  }
  for (const platform of [
    "darwin-aarch64",
    "darwin-x86_64",
    "windows-x86_64",
    "linux-x86_64",
  ]) {
    assert.match(runbook, new RegExp(`^${platform}$`, "m"));
  }
  assert.doesNotMatch(runbook, /windows-aarch64|linux-aarch64|universal-apple/);
});

test("repository policy and automation never direct product traffic upstream", async () => {
  for (const path of POLICY_DOCUMENTS) {
    const contents = await source(path);
    assert.doesNotMatch(
      contents,
      /farion1231|\bCC Switch\b/i,
      `${path} still presents upstream as the product owner`,
    );
  }

  for (const path of [".github/FUNDING.yml", ".github/CODEOWNERS"]) {
    const contents = await optionalSource(path);
    if (contents !== null) {
      assert.doesNotMatch(
        contents,
        /farion1231/i,
        `${path} still names upstream`,
      );
    }
  }

  const workflowNames = await readdir(new URL(".github/workflows/", root));
  for (const name of workflowNames.filter((path) => /\.ya?ml$/.test(path))) {
    const workflow = await source(`.github/workflows/${name}`);
    assert.doesNotMatch(
      workflow,
      /ccswitch\.io|cc-switch-releases|farion1231\/cc-switch/i,
      `${name} still targets upstream infrastructure`,
    );
    assert.doesNotMatch(
      workflow,
      /reviewing PRs for cc-switch/i,
      `${name} still instructs automation to review the upstream product`,
    );
    if (!new Set(["release.yml", "sync-r2.yml"]).has(name)) {
      assert.doesNotMatch(
        workflow,
        /^\s{2}release:\s*$/m,
        `${name} reacts to publication outside the protected release workflow`,
      );
    }
  }

  const ci = await source(".github/workflows/ci.yml");
  for (const releaseRelevantPath of ['"*.md"', '"docs/**"', '".github/**"']) {
    assert.ok(
      ci
        .split(/\r?\n/)
        .some((line) => line.trim() === `- ${releaseRelevantPath}`),
      `CI does not run release documentation checks for ${releaseRelevantPath}`,
    );
  }
  assert.match(ci, /actionlint_1\.7\.12_linux_amd64\.tar\.gz/);
  assert.match(
    ci,
    /8aca8db96f1b94770f1b0d72b6dddcb1ebb8123cb3712530b08cc387b349a3d8/,
  );
  assert.match(ci, /sha256sum -c/);
});

// The .gitignore rules below are the first line of defence, but `git add -f`
// bypasses them and a tracked file stays tracked regardless. This check is the
// one that actually holds: it reads what git would publish.
test("the tracked tree carries no credentials or assistant configuration", async () => {
  const { stdout } = await promisify(execFile)("git", ["ls-files", "-z"], {
    cwd: root.pathname,
    maxBuffer: 32 * 1024 * 1024,
  });
  const tracked = stdout.split("\0").filter(Boolean);
  assert.ok(tracked.length > 0, "git ls-files returned nothing");

  const forbiddenPattern =
    /(^|\/)(\.env(\..+)?|\.npmrc|\.netrc|\.mcp\.json|credentials\.json|secrets\.json|id_rsa.*|id_ed25519.*)$|\.(p12|pfx|p8|pem|key|keystore|jks|mobileprovision|provisionprofile|ppk)$/i;
  const assistantPattern =
    /(^|\/)(CLAUDE(\.local)?\.md|AGENTS\.md|GEMINI\.md|CODEBUDDY\.md|\.cursorrules|\.windsurfrules|copilot-instructions\.md)$|(^|\/)\.(claude|codex|gemini|cursor|continue|windsurf|roo|superpowers|spec-workflow|cc-switch)\//i;

  const credentials = tracked.filter((path) => forbiddenPattern.test(path));
  assert.deepEqual(
    credentials,
    [],
    "credential or environment files are tracked and would be published",
  );

  const assistant = tracked.filter((path) => assistantPattern.test(path));
  assert.deepEqual(
    assistant,
    [],
    "assistant configuration is tracked and would be published",
  );
});
