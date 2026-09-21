/*
 * The page reads a manifest the release workflow publishes to the product's own
 * distribution host, not the GitHub API.
 *
 * Two reasons. The installers are already mirrored to `dl.aimanager.tools`, so
 * asking GitHub for their sizes would put the slowest hop in front of every
 * visitor, and that hop is slowest exactly where most of our readers are. And
 * the unauthenticated GitHub API allows sixty calls an hour per address, which
 * a shared office or a campus can exhaust without anyone noticing.
 *
 * GitHub stays as the stated fallback: a link at the bottom of the page, and
 * the whole page's answer when the manifest cannot be read.
 *
 * All display copy comes from `i18n.js`. Nothing here is written in English
 * except the platform names, which are proper nouns in every language.
 */
const DISTRIBUTION = "https://dl.aimanager.tools/ai-manager";
const STABLE_MANIFEST = `${DISTRIBUTION}/download.json`;
const STAGING_MANIFEST = `${DISTRIBUTION}/staging/download.json`;
const RELEASES_PAGE = "https://github.com/OnlistTeam/ai-manager/releases";

/**
 * Installers, in the order they are listed when no platform is detected. The
 * `id` is what the copy is keyed by; the manifest identifies the same file by
 * the platform/arch/kind triple.
 */
const PLATFORMS = [
  {
    id: "macos-arm64",
    name: "macOS &middot; Apple Silicon",
    platform: "macos",
    arch: "arm64",
    kind: "dmg",
  },
  {
    id: "macos-x64",
    name: "macOS &middot; Intel",
    platform: "macos",
    arch: "x64",
    kind: "dmg",
  },
  {
    id: "windows-x64",
    name: "Windows &middot; x64",
    platform: "windows",
    arch: "x64",
    kind: "msi",
  },
  {
    id: "linux-appimage",
    name: "Linux &middot; x64 AppImage",
    platform: "linux",
    arch: "x64",
    kind: "appimage",
  },
  {
    id: "linux-deb",
    name: "Linux &middot; x64 deb",
    platform: "linux",
    arch: "x64",
    kind: "deb",
  },
];

const t = (key) => window.pageI18n.t(key);

function megabytes(bytes) {
  return `${(bytes / 1_000_000).toFixed(1)} MB`;
}

function escapeAttribute(value) {
  return String(value).replaceAll("&", "&amp;").replaceAll('"', "&quot;");
}

function fill(template, values) {
  return Object.entries(values).reduce(
    (text, [name, value]) => text.replaceAll(`{${name}}`, value),
    template,
  );
}

/**
 * What the visitor is running. A phone or tablet gets `mobile`: offering a
 * desktop installer there is worse than offering nothing, because the download
 * appears to succeed and then cannot be opened.
 */
function detectPlatform() {
  const agent = navigator.userAgent;
  if (/Android|iPhone|iPad|iPod/i.test(agent)) return "mobile";

  const hint = navigator.userAgentData?.platform;
  if (hint === "macOS") return "macos";
  if (hint === "Windows") return "windows";
  if (hint === "Linux") return "linux";

  if (/Mac/i.test(agent)) return "macos";
  if (/Win/i.test(agent)) return "windows";
  if (/Linux|X11/i.test(agent)) return "linux";
  return null;
}

/**
 * The CPU architecture, when the browser will say. Only Chromium answers this;
 * Safari and Firefox return null, and a macOS visitor is then offered both
 * builds rather than being handed a binary that will not start.
 */
async function detectArchitecture() {
  try {
    const data = navigator.userAgentData;
    if (!data?.getHighEntropyValues) return null;
    const { architecture } = await data.getHighEntropyValues(["architecture"]);
    if (architecture === "arm") return "arm64";
    if (architecture === "x86") return "x64";
  } catch {
    // An unavailable hint is not an error; it just means we ask the visitor.
  }
  return null;
}

/**
 * A manifest is only usable if it describes every installer the page offers.
 * A partial one is treated as no manifest at all rather than as a page with
 * holes in it.
 */
function installersFrom(manifest) {
  if (!manifest || !Array.isArray(manifest.files)) return null;
  const byId = new Map();
  for (const platform of PLATFORMS) {
    const file = manifest.files.find(
      (entry) =>
        entry.platform === platform.platform &&
        entry.arch === platform.arch &&
        entry.kind === platform.kind &&
        typeof entry.url === "string" &&
        entry.url.startsWith(`${DISTRIBUTION}/`) &&
        Number.isFinite(entry.size) &&
        entry.size > 0,
    );
    if (!file) return null;
    byId.set(platform.id, { ...platform, file });
  }
  return byId;
}

function primaryButton(entry, label) {
  return `
    <a class="primary-download" href="${escapeAttribute(entry.file.url)}">
      <span>${label}</span>
      <span class="meta">${megabytes(entry.file.size)}</span>
    </a>`;
}

function secondaryLine(entry, sentenceKey, linkKey) {
  const link = `<a href="${escapeAttribute(entry.file.url)}">${t(linkKey)}</a>`;
  return `<p class="arch-hint">${fill(t(sentenceKey), { link })}</p>`;
}

function renderMac(container, installers, arch) {
  const armBuild = installers.get("macos-arm64");
  const intelBuild = installers.get("macos-x64");

  if (arch === "arm64") {
    container.innerHTML =
      primaryButton(armBuild, t("dl.macArm")) +
      secondaryLine(intelBuild, "dl.archHintOther", "dl.archHintOtherLink");
    return;
  }
  if (arch === "x64") {
    container.innerHTML =
      primaryButton(intelBuild, t("dl.macIntel")) +
      secondaryLine(armBuild, "dl.archHintIsIntel", "dl.archHintIsIntelLink");
    return;
  }

  container.innerHTML = `
    <div class="arch-choice">
      ${primaryButton(armBuild, t("dl.macArm"))}
      ${primaryButton(intelBuild, t("dl.macIntel"))}
    </div>
    <p class="arch-hint">${t("dl.archHintBoth")}</p>`;
}

function renderRecommended(container, installers, platform, arch) {
  container.hidden = false;

  if (platform === "mobile") {
    container.innerHTML = `<p class="arch-hint">${t("dl.mobile")}</p>`;
    return;
  }
  if (platform === "macos") {
    renderMac(container, installers, arch);
    return;
  }
  if (platform === "windows" || platform === "linux") {
    const entry = installers.get(
      platform === "windows" ? "windows-x64" : "linux-appimage",
    );
    const label = platform === "windows" ? t("dl.windows") : t("dl.linux");
    container.innerHTML = primaryButton(entry, label);
    return;
  }

  container.hidden = true;
}

function renderAllPlatforms(container, heading, installers) {
  const rows = [];
  for (const platform of PLATFORMS) {
    const entry = installers.get(platform.id);
    rows.push(`
      <div class="platform-row">
        <span class="name">
          ${platform.name}
          <span class="hint">${t(`dl.note.${platform.id}`)}</span>
        </span>
        <span class="size">${megabytes(entry.file.size)}</span>
        <span class="links">
          <a href="${escapeAttribute(entry.file.url)}">${t("dl.download")}</a>
          <button type="button" class="checksum" data-digest="${escapeAttribute(entry.file.sha256)}">
            ${t("dl.checksum")}
          </button>
        </span>
      </div>`);
  }
  container.innerHTML = rows.join("");
  container.hidden = false;
  heading.hidden = false;
}

/**
 * The digest travels in the manifest, so checking a download no longer means
 * downloading a second file to read one line out of it.
 */
function wireChecksumButtons(container) {
  for (const button of container.querySelectorAll("button.checksum")) {
    button.addEventListener("click", async () => {
      try {
        await navigator.clipboard.writeText(button.dataset.digest);
      } catch {
        // No clipboard permission: show the digest so it can be selected.
        button.replaceWith(
          Object.assign(document.createElement("code"), {
            className: "digest",
            textContent: button.dataset.digest,
          }),
        );
        return;
      }
      const label = button.textContent;
      button.textContent = t("dl.hashCopied");
      button.disabled = true;
      setTimeout(() => {
        button.textContent = label;
        button.disabled = false;
      }, 1500);
    });
  }
}

async function readManifest(url) {
  const response = await fetch(url, {
    headers: { Accept: "application/json" },
  });
  if (!response.ok) return null;
  return response.json();
}

/**
 * Stable first. A prerelease is only offered when no stable release exists,
 * which is the honest answer during the period before the first one.
 */
async function newestManifest() {
  for (const url of [STABLE_MANIFEST, STAGING_MANIFEST]) {
    try {
      const manifest = await readManifest(url);
      if (installersFrom(manifest)) return manifest;
    } catch {
      // A network failure on the stable manifest should still let the staging
      // one answer, and the caller handles both being unreachable.
    }
  }
  return null;
}

/**
 * Looked up once, so switching language re-renders rather than fetching again.
 */
const state = {
  manifest: undefined,
  platform: detectPlatform(),
  arch: null,
};

function render() {
  const line = document.getElementById("release-line");
  const recommended = document.getElementById("recommended");
  const heading = document.getElementById("platforms-heading");
  const platforms = document.getElementById("all-platforms");
  const fallback = document.getElementById("fallback");
  const source = document.getElementById("source-note");
  const prereleaseNote = document.getElementById("prerelease-note");

  if (state.manifest === undefined) return;

  const installers = installersFrom(state.manifest);
  if (!installers) {
    line.hidden = true;
    fallback.hidden = false;
    return;
  }

  renderAllPlatforms(platforms, heading, installers);
  wireChecksumButtons(platforms);
  renderRecommended(recommended, installers, state.platform, state.arch);

  const published = new Date(state.manifest.publishedAt).toLocaleDateString(
    window.pageI18n.dateLocale(),
    { year: "numeric", month: "long", day: "numeric" },
  );
  const notes = `<a href="${escapeAttribute(state.manifest.releaseNotes || RELEASES_PAGE)}">${t("dl.releaseNotes")}</a>`;
  line.hidden = false;
  line.innerHTML = `${fill(t("dl.versionLine"), {
    version: `<strong>${escapeAttribute(state.manifest.tag)}</strong>`,
    date: published,
  })} &middot; ${notes}`;

  source.hidden = false;
  source.innerHTML = fill(t("dl.source"), {
    link: `<a href="${RELEASES_PAGE}">${t("dl.sourceLink")}</a>`,
  });

  prereleaseNote.hidden = state.manifest.channel !== "staging";
}

async function start() {
  window.pageI18n.onChange(render);
  state.arch = await detectArchitecture();
  state.manifest = await newestManifest();
  render();
}

void start();
