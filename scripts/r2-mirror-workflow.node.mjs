import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";
import test from "node:test";

const root = new URL("../", import.meta.url);

async function source(path) {
  return readFile(new URL(path, root), "utf8");
}

test("distribution follows published releases, never a tag push", async () => {
  const workflow = await source(".github/workflows/sync-r2.yml");

  assert.match(workflow, /^on:\n\s+release:\n\s+types: \[published\]/m);
  assert.match(workflow, /workflow_call:[\s\S]*tag:[\s\S]*required: true/);
  assert.match(workflow, /workflow_dispatch:/);
  assert.doesNotMatch(workflow, /^\s+push:\s*$|^\s+tags:\s*$/m);
  assert.match(workflow, /^\s{4}environment: release$/m);
  assert.match(
    workflow,
    /test "\$GITHUB_REPOSITORY" = "OnlistTeam\/ai-manager"/,
  );
  assert.match(
    workflow,
    /CLOUDFLARE_API_TOKEN: \$\{\{ secrets\.CLOUDFLARE_API_TOKEN \}\}/,
  );
  assert.match(
    workflow,
    /CLOUDFLARE_ACCOUNT_ID: \$\{\{ vars\.CLOUDFLARE_ACCOUNT_ID \}\}/,
  );
  assert.doesNotMatch(workflow, /CC[- ]Switch|ccswitch\.io/i);
});

test("the protected release calls distribution explicitly after publication", async () => {
  const workflow = await source(".github/workflows/release.yml");
  assert.match(
    workflow,
    /sync-distribution:[\s\S]*needs: publish-release[\s\S]*uses: \.\/\.github\/workflows\/sync-r2\.yml[\s\S]*tag: \$\{\{ inputs\.tag \}\}[\s\S]*secrets: inherit/,
  );
});

test("stable and staging roots are isolated and published after versioned assets", async () => {
  const workflow = await source(".github/workflows/sync-r2.yml");
  const versionedAssets = workflow.indexOf(
    "Upload immutable versioned assets first",
  );
  const stagingRoot = workflow.indexOf(
    "Recheck latest prerelease and publish staging root last",
  );
  const stableRoot = workflow.indexOf(
    "Recheck latest stable release and publish stable root last",
  );
  const publicProbe = workflow.indexOf("Verify the public manifests");

  assert(
    versionedAssets >= 0 &&
      stagingRoot > versionedAssets &&
      stableRoot > stagingRoot &&
      publicProbe > stableRoot,
  );
  assert.match(
    workflow,
    /channel="staging"[\s\S]*root_key="staging\/latest\.json"/,
  );
  assert.match(workflow, /channel="stable"[\s\S]*root_key="latest\.json"/);
  assert.match(
    workflow,
    /map\(select\(\.draft == false and \.prerelease == true\)\)[\s\S]*sort_by\(\.published_at\)/,
  );
  assert.match(workflow, /repos\/\$GITHUB_REPOSITORY\/releases\/latest/);
  assert.match(
    workflow,
    /if: steps\.release\.outputs\.is_latest == 'true' && steps\.release\.outputs\.channel == 'staging'/,
  );
  assert.match(
    workflow,
    /if: steps\.release\.outputs\.is_latest == 'true' && steps\.release\.outputs\.channel == 'stable'/,
  );
  assert.match(
    workflow,
    /\$R2_BUCKET\/\$R2_PREFIX\/\$TAG\/\$name[\s\S]*max-age=31536000, immutable/,
  );
  assert.match(workflow, /latest-distribution\.json/);
  assert.match(
    workflow,
    /cmp --silent "\$expected" "\$observed"/,
  );
  assert.doesNotMatch(workflow, /r2 object delete|s3 rm|r2 bucket delete/);
});

test("the download manifest rides the same channel gates as the updater manifest", async () => {
  const workflow = await source(".github/workflows/sync-r2.yml");

  // Built from the same validated asset directory, in the same run.
  assert.match(
    workflow,
    /node scripts\/build-download-manifest\.mjs[\s\S]*--base-url "\$PUBLIC_BASE_URL"[\s\S]*--output download-manifest\.json/,
  );

  // Published only inside the two steps that already re-check whether this
  // release is still the newest on its channel, so a late upload can never
  // point the page at an older build than the updater.
  const staging = workflow.indexOf(
    "Recheck latest prerelease and publish staging root last",
  );
  const stable = workflow.indexOf(
    "Recheck latest stable release and publish stable root last",
  );
  const probe = workflow.indexOf("Verify the public manifests");
  const stagingUpload = workflow.indexOf(
    "$R2_BUCKET/$R2_PREFIX/staging/download.json",
  );
  const stableUpload = workflow.indexOf("$R2_BUCKET/$R2_PREFIX/download.json");

  assert(
    stagingUpload > staging && stagingUpload < stable,
    "the staging download manifest belongs to the staging step",
  );
  assert(
    stableUpload > stable && stableUpload < probe,
    "the stable download manifest belongs to the stable step",
  );

  // And it is read back from the public host before the run is called a success.
  assert.match(
    workflow,
    /verify "download manifest"[\s\S]*download-manifest\.json observed-download\.json/,
  );
});

test("stable distribution uses exactly one product-owned endpoint", async () => {
  const [workflow, config, contract] = await Promise.all([
    source(".github/workflows/sync-r2.yml"),
    source("src-tauri/tauri.conf.json"),
    source("scripts/release-contract.mjs"),
  ]);

  assert.match(workflow, /R2_BUCKET: aimanager-releases/);
  assert.match(workflow, /R2_PREFIX: ai-manager/);
  assert.match(
    workflow,
    /PUBLIC_BASE_URL: https:\/\/dl\.aimanager\.tools\/ai-manager/,
  );
  // A single product channel means no second mirror and no second credential set.
  assert.doesNotMatch(workflow, /BACKUP_|AWS_ACCESS_KEY_ID|aws s3 cp/);
  assert.deepEqual(JSON.parse(config).plugins.updater.endpoints, [
    "https://dl.aimanager.tools/ai-manager/latest.json",
  ]);
  assert.match(contract, /PRODUCT_UPDATE_HOST = "dl\.aimanager\.tools"/);
  assert.match(contract, /PRODUCT_UPDATE_PATH = "\/ai-manager\/latest\.json"/);
  assert.match(contract, /endpoints\.length !== 1/);
});
