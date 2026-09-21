/*
 * The page reads the release list at request time instead of baking asset URLs
 * in, so publishing a new version never requires redeploying this site. The
 * only cost is that GitHub has to be reachable; when it is not, the fallback
 * link still gets the visitor to the same files.
 */
const RELEASES_API =
  "https://api.github.com/repos/OnlistTeam/ai-manager/releases";
const RELEASES_PAGE = "https://github.com/OnlistTeam/ai-manager/releases";

/**
 * Installers, in the order they are listed when no platform is detected.
 * `match` identifies the installer asset; the checksum asset is always the
 * installer's name plus `.sha256`.
 */
const PLATFORMS = [
  {
    id: "macos-arm64",
    name: "macOS &middot; Apple Silicon",
    suffix: "-macOS-arm64.dmg",
  },
  {
    id: "macos-x64",
    name: "macOS &middot; Intel",
    suffix: "-macOS-x64.dmg",
  },
  {
    id: "windows-x64",
    name: "Windows &middot; x64",
    suffix: "-Windows-x64.msi",
  },
  {
    id: "linux-appimage",
    name: "Linux &middot; x64 AppImage",
    suffix: "-Linux-x64.AppImage",
  },
  {
    id: "linux-deb",
    name: "Linux &middot; x64 deb",
    suffix: "-Linux-x64.deb",
  },
];

function megabytes(bytes) {
  return `${(bytes / 1_000_000).toFixed(1)} MB`;
}

function escapeAttribute(value) {
  return String(value).replaceAll("&", "&amp;").replaceAll('"', "&quot;");
}

/**
 * What the visitor is most likely running. Browsers do not report CPU
 * architecture reliably, so a macOS visitor is offered both builds rather than
 * guessing wrong and handing an Intel user a binary that will not start.
 */
function detectPlatform() {
  const hint = navigator.userAgentData?.platform ?? "";
  const agent = `${hint} ${navigator.userAgent}`.toLowerCase();

  if (agent.includes("mac")) return "macos";
  if (agent.includes("win")) return "windows";
  if (agent.includes("linux") || agent.includes("x11")) return "linux";
  return null;
}

function installersFrom(release) {
  const byId = new Map();
  for (const platform of PLATFORMS) {
    const installer = release.assets.find((asset) =>
      asset.name.endsWith(platform.suffix),
    );
    if (!installer) continue;
    const checksum = release.assets.find(
      (asset) => asset.name === `${installer.name}.sha256`,
    );
    byId.set(platform.id, { ...platform, installer, checksum });
  }
  return byId;
}

function primaryButton(entry, label) {
  return `
    <a class="primary-download" href="${escapeAttribute(entry.installer.browser_download_url)}">
      <span>${label}</span>
      <span class="meta">${megabytes(entry.installer.size)}</span>
    </a>`;
}

function renderRecommended(container, installers, platform) {
  const macArm = installers.get("macos-arm64");
  const macIntel = installers.get("macos-x64");

  if (platform === "macos" && macArm && macIntel) {
    container.innerHTML = `
      <div class="arch-choice">
        ${primaryButton(macArm, "Download for Apple Silicon")}
        ${primaryButton(macIntel, "Download for Intel")}
      </div>
      <p class="arch-hint">
        Not sure which? Apple menu &rsaquo; About This Mac. A chip named
        "Apple M-something" is Apple Silicon.
      </p>`;
    container.hidden = false;
    return;
  }

  const single =
    platform === "windows"
      ? installers.get("windows-x64")
      : platform === "linux"
        ? installers.get("linux-appimage")
        : null;

  if (!single) return;

  const label =
    platform === "windows" ? "Download for Windows" : "Download the AppImage";
  container.innerHTML = primaryButton(single, label);
  container.hidden = false;
}

function renderAllPlatforms(container, installers) {
  const rows = [];
  for (const platform of PLATFORMS) {
    const entry = installers.get(platform.id);
    if (!entry) continue;
    const checksum = entry.checksum
      ? `<a href="${escapeAttribute(entry.checksum.browser_download_url)}">SHA-256</a>`
      : "";
    rows.push(`
      <div class="platform-row">
        <span class="name">${platform.name}</span>
        <span class="size">${megabytes(entry.installer.size)}</span>
        <span class="links">
          <a href="${escapeAttribute(entry.installer.browser_download_url)}">Download</a>
          ${checksum}
        </span>
      </div>`);
  }

  if (rows.length === 0) return false;
  container.innerHTML = rows.join("");
  container.hidden = false;
  return true;
}

async function newestRelease() {
  const response = await fetch(RELEASES_API, {
    headers: { Accept: "application/vnd.github+json" },
  });
  if (!response.ok) throw new Error(`GitHub replied ${response.status}`);
  const releases = await response.json();
  // Releases come back newest first. Drafts are never visible to an
  // unauthenticated caller, so a prerelease is a legitimate newest release
  // while no stable one exists.
  return releases.find((release) => !release.draft) ?? null;
}

async function render() {
  const line = document.getElementById("release-line");
  const recommended = document.getElementById("recommended");
  const platforms = document.getElementById("all-platforms");
  const fallback = document.getElementById("fallback");
  const prereleaseNote = document.getElementById("prerelease-note");

  let release;
  try {
    release = await newestRelease();
  } catch {
    release = null;
  }

  if (!release) {
    line.hidden = true;
    fallback.hidden = false;
    return;
  }

  const installers = installersFrom(release);
  if (!renderAllPlatforms(platforms, installers)) {
    line.hidden = true;
    fallback.hidden = false;
    return;
  }

  renderRecommended(recommended, installers, detectPlatform());

  // The page is English, so the date is formatted in English rather than in
  // whatever locale the browser happens to prefer.
  const published = new Date(release.published_at).toLocaleDateString("en", {
    year: "numeric",
    month: "long",
    day: "numeric",
  });
  line.innerHTML = `
    Version <strong>${escapeAttribute(release.tag_name)}</strong>,
    published ${published} &middot;
    <a href="${escapeAttribute(release.html_url || RELEASES_PAGE)}">release notes</a>`;

  prereleaseNote.hidden = !release.prerelease;
}

void render();
