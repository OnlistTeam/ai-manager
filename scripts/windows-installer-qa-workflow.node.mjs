import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";
import test from "node:test";

const workflowPath = new URL(
  "../.github/workflows/windows-installer-qa.yml",
  import.meta.url,
);

async function workflow() {
  return readFile(workflowPath, "utf8");
}

test("Windows installer QA is manual, isolated, and never publishes its artifact", async () => {
  const source = await workflow();

  assert.match(source, /^\s{2}workflow_dispatch:\s*$/m);
  assert.doesNotMatch(source, /^\s{2}(?:push|pull_request|schedule):\s*$/m);
  assert.match(source, /^permissions:\n\s{2}contents: read$/m);
  assert.match(source, /runs-on: windows-2025/);
  assert.doesNotMatch(source, /^\s{4}environment:\s*release\s*$/m);
  assert.doesNotMatch(source, /\bsecrets\./);
  assert.doesNotMatch(source, /release-assets|softprops\/action-gh-release/);
  assert.match(source, /path: windows-qa-audit\//);
  assert.match(source, /PublicTrust = \$false/);
  assert.match(source, /ephemeral-runner-only-not-public-distribution/);
});

test("QA builds the same unsigned installer the release workflow publishes", async () => {
  const source = await workflow();

  // Windows ships without a platform code signature, so QA must exercise the
  // artifact users actually receive rather than a locally trusted stand-in.
  assert.doesNotMatch(source, /New-SelfSignedCertificate|CodeSigningCert/i);
  assert.doesNotMatch(source, /signtool|SIGNTOOL_PATH/i);
  assert.doesNotMatch(source, /Get-AuthenticodeSignature|Authenticode/i);
  assert.doesNotMatch(source, /signCommand|sign-windows\.ps1/i);
  assert.doesNotMatch(source, /Cert:\\CurrentUser/);
  assert.doesNotMatch(
    source,
    /WINDOWS_(?:CERTIFICATE|SIGNING_PROVIDER|TIMESTAMP_URL)/,
  );
  assert.doesNotMatch(source, /timestamp\.digicert\.com|RFC 3161/i);
});

test("native Windows QA verifies the exact MSI payload before install-launch-uninstall", async () => {
  const source = await workflow();
  const build = source.indexOf("--no-bundle");
  const bundle = source.indexOf("--bundles msi");
  const extract = source.indexOf("msiexec.exe /a");
  const marker = source.indexOf("verify-windows-bundle-marker.mjs");
  const install = source.indexOf("msiexec.exe /i");
  const launch = source.indexOf("Start-Process -FilePath $installedExe");
  const uninstall = source.indexOf("msiexec.exe /x");

  assert.match(source, /createUpdaterArtifacts":false/);
  assert.match(
    source,
    /verify-windows-bundle-marker\.mjs[\s\S]{0,300}\n\s+msi \|/,
  );
  assert.ok(build >= 0 && bundle > build);
  assert.ok(extract > bundle && marker > extract);
  assert.ok(install > marker && launch > install && uninstall > launch);
  assert.match(source, /Installed executable remains after MSI uninstall/);
});
