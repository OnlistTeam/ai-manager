/*
 * The release workflow publishes this manifest and every installer to our
 * download host. The page never sends a visitor to a third-party download
 * unless the mirror itself is temporarily unavailable.
 */
const DISTRIBUTION = "https://dl.aimanager.tools/ai-manager";
const STABLE_MANIFEST = `${DISTRIBUTION}/download.json`;
const STAGING_MANIFEST = `${DISTRIBUTION}/staging/download.json`;
const RELEASES_PAGE = "https://github.com/OnlistTeam/ai-manager/releases";

const PLATFORMS = [
  {
    id: "macos-arm64",
    group: "macos",
    name: "macOS · Apple Silicon",
    platform: "macos",
    arch: "arm64",
    kind: "dmg",
    nameKey: "dl.macArmName",
    noteKey: "dl.note.macos-arm64",
  },
  {
    id: "macos-x64",
    group: "macos",
    name: "macOS · Intel",
    platform: "macos",
    arch: "x64",
    kind: "dmg",
    nameKey: "dl.macIntelName",
    noteKey: "dl.note.macos-x64",
  },
  {
    id: "windows-x64",
    group: "windows",
    name: "Windows · x64",
    platform: "windows",
    arch: "x64",
    kind: "msi",
    nameKey: "dl.windowsName",
    noteKey: "dl.note.windows-x64",
  },
  {
    id: "linux-appimage",
    group: "linux",
    name: "Linux · AppImage",
    platform: "linux",
    arch: "x64",
    kind: "appimage",
    nameKey: "dl.appImageName",
    noteKey: "dl.note.linux-appimage",
  },
  {
    id: "linux-deb",
    group: "linux",
    name: "Linux · deb",
    platform: "linux",
    arch: "x64",
    kind: "deb",
    nameKey: "dl.debName",
    noteKey: "dl.note.linux-deb",
  },
];

/*
 * `noticeKey` is the one thing a person needs to know before double-clicking
 * the file they just downloaded — what the system will say, and whether that
 * is expected. It used to be four cards of prose and shell commands in their
 * own section, which made platform caveats the largest thing on a page whose
 * job is a download button. Here it reaches the reader who selected that
 * platform, and nobody else.
 */
const PLATFORM_GROUPS = {
  macos: {
    titleKey: "dl.macosCardTitle",
    bodyKey: "dl.macosCardBody",
    noticeKey: "dl.macosNotice",
    ids: ["macos-arm64", "macos-x64"],
  },
  windows: {
    titleKey: "dl.windowsCardTitle",
    bodyKey: "dl.windowsCardBody",
    noticeKey: "dl.windowsNotice",
    ids: ["windows-x64"],
  },
  linux: {
    titleKey: "dl.linuxCardTitle",
    bodyKey: "dl.linuxCardBody",
    noticeKey: "dl.linuxNotice",
    ids: ["linux-appimage", "linux-deb"],
  },
};

const t = (key) => window.pageI18n.t(key);

function megabytes(bytes) {
  return `${(bytes / 1_000_000).toFixed(1)} MB`;
}

function escapeHtml(value) {
  return String(value)
    .replaceAll("&", "&amp;")
    .replaceAll("<", "&lt;")
    .replaceAll(">", "&gt;")
    .replaceAll('"', "&quot;")
    .replaceAll("'", "&#39;");
}

function escapeAttribute(value) {
  return escapeHtml(value);
}

function fill(template, values) {
  return Object.entries(values).reduce(
    (text, [name, value]) => text.replaceAll(`{${name}}`, value),
    template,
  );
}

function detectPlatform() {
  const agent =
    `${navigator.userAgentData?.platform ?? ""} ${navigator.userAgent}`.toLowerCase();
  if (/android|iphone|ipad|ipod/.test(agent)) return "mobile";
  if (agent.includes("mac")) return "macos";
  if (agent.includes("win")) return "windows";
  if (agent.includes("linux") || agent.includes("x11")) return "linux";
  return null;
}

async function detectArchitecture() {
  try {
    const data = navigator.userAgentData;
    if (!data?.getHighEntropyValues) return null;
    const { architecture } = await data.getHighEntropyValues(["architecture"]);
    if (architecture === "arm") return "arm64";
    if (architecture === "x86") return "x64";
  } catch {
    // Browsers may withhold the high-entropy hint; offer both Mac builds.
  }
  return null;
}

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
        entry.size > 0 &&
        typeof entry.sha256 === "string" &&
        entry.sha256.length === 64,
    );
    if (!file) return null;
    byId.set(platform.id, { ...platform, file });
  }
  return byId;
}

function primaryButton(entry, label) {
  return `
    <a class="primary-download" href="${escapeAttribute(entry.file.url)}">
      <span class="download-icon" aria-hidden="true">↓</span>
      <span>${escapeHtml(label)}</span>
      <span class="meta">${megabytes(entry.file.size)}</span>
    </a>`;
}

function renderRecommended(container, installers, platform, arch) {
  container.innerHTML = "";
  container.hidden = false;

  if (platform === "mobile") {
    container.innerHTML = `<p class="hero-hint">${t("dl.mobile")}</p>`;
    return;
  }

  if (platform === "macos" && !arch) {
    const arm = installers.get("macos-arm64");
    const intel = installers.get("macos-x64");
    container.innerHTML = `
      <div class="hero-choice">
        ${primaryButton(arm, t("dl.macArm"))}
        ${primaryButton(intel, t("dl.macIntel"))}
      </div>
      <p class="hero-hint">${t("dl.archHintBoth")}</p>`;
    return;
  }

  const id =
    platform === "macos"
      ? `macos-${arch}`
      : platform === "windows"
        ? "windows-x64"
        : platform === "linux"
          ? "linux-appimage"
          : null;
  const entry = id ? installers.get(id) : null;
  if (!entry) {
    container.hidden = true;
    return;
  }

  const labelKey =
    platform === "macos"
      ? arch === "arm64"
        ? "dl.macArm"
        : "dl.macIntel"
      : platform === "windows"
        ? "dl.windows"
        : "dl.linux";
  container.innerHTML = primaryButton(entry, t(labelKey));
}

function fileRow(entry) {
  const label = t(entry.nameKey) || entry.name;
  const note = t(entry.noteKey);
  const digest = escapeAttribute(entry.file.sha256);
  return `
    <div class="file-row">
      <div>
        <span class="file-name">${escapeHtml(label)}</span>
        <span class="file-note">${escapeHtml(note)}</span>
      </div>
      <div class="file-actions">
        <a class="file-download" href="${escapeAttribute(entry.file.url)}">
          <span class="download-icon" aria-hidden="true">↓</span>
          <span>${t("dl.download")}</span>
          <span class="meta">${megabytes(entry.file.size)}</span>
        </a>
        <button type="button" class="checksum" data-digest="${digest}">
          ${t("dl.checksum")}
        </button>
      </div>
    </div>`;
}

function renderPlatformPanel(container, installers, group) {
  const config = PLATFORM_GROUPS[group];
  if (!config) return;
  const entries = config.ids.map((id) => installers.get(id)).filter(Boolean);
  container.innerHTML = `
    <article class="download-card">
      <header class="download-card-header">
        <div>
          <h3>${t(config.titleKey)}</h3>
          <p>${t(config.bodyKey)}</p>
        </div>
      </header>
      <div class="file-list">${entries.map(fileRow).join("")}</div>
      <p class="platform-notice">${t(config.noticeKey)}</p>
    </article>`;
  wireChecksumButtons(container);
}

function wireChecksumButtons(container) {
  for (const button of container.querySelectorAll("button.checksum")) {
    button.addEventListener("click", async () => {
      const digest = button.dataset.digest;
      try {
        await navigator.clipboard.writeText(digest);
        button.textContent = t("dl.hashCopied");
        button.disabled = true;
        setTimeout(() => {
          button.textContent = t("dl.checksum");
          button.disabled = false;
        }, 1500);
      } catch {
        button.insertAdjacentHTML(
          "afterend",
          `<code class="digest">${escapeHtml(digest)}</code>`,
        );
        button.disabled = true;
      }
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

async function newestManifest() {
  for (const url of [STABLE_MANIFEST, STAGING_MANIFEST]) {
    try {
      const manifest = await readManifest(url);
      if (installersFrom(manifest)) return manifest;
    } catch {
      // The caller renders the recovery message if both manifests fail.
    }
  }
  return null;
}

const detectedPlatform = detectPlatform();
const state = {
  manifest: undefined,
  platform: detectedPlatform,
  arch: null,
  selectedPlatform:
    detectedPlatform === "windows" || detectedPlatform === "linux"
      ? detectedPlatform
      : "macos",
};

function updateTabs() {
  for (const button of document.querySelectorAll("[data-platform-tab]")) {
    button.setAttribute(
      "aria-selected",
      String(button.dataset.platformTab === state.selectedPlatform),
    );
  }
}

function render() {
  const line = document.getElementById("release-line");
  const recommended = document.getElementById("recommended");
  const panel = document.getElementById("platform-panel");
  const fallback = document.getElementById("fallback");
  const source = document.getElementById("source-note");
  const prereleaseNote = document.getElementById("prerelease-note");

  updateTabs();
  if (state.manifest === undefined) return;

  const installers = installersFrom(state.manifest);
  if (!installers) {
    line.hidden = true;
    recommended.hidden = true;
    panel.hidden = true;
    source.hidden = true;
    fallback.hidden = false;
    return;
  }

  const published = new Date(state.manifest.publishedAt).toLocaleDateString(
    window.pageI18n.dateLocale(),
    { year: "numeric", month: "long", day: "numeric" },
  );
  line.hidden = false;
  line.innerHTML = fill(t("dl.versionLine"), {
    version: `<strong>${escapeHtml(state.manifest.tag)}</strong>`,
    date: published,
  });

  renderRecommended(recommended, installers, state.platform, state.arch);
  renderPlatformPanel(panel, installers, state.selectedPlatform);
  panel.hidden = false;

  source.hidden = false;
  source.innerHTML =
    fill(t("dl.source"), { mirror: `<code>dl.aimanager.tools</code>` }) +
    ` · <a href="${escapeAttribute(state.manifest.releaseNotes || RELEASES_PAGE)}">${t("dl.githubAlternative")}</a>`;
  prereleaseNote.hidden = state.manifest.channel !== "staging";
  fallback.hidden = true;
}

function wirePlatformTabs() {
  for (const button of document.querySelectorAll("[data-platform-tab]")) {
    button.addEventListener("click", () => {
      state.selectedPlatform = button.dataset.platformTab;
      render();
    });
  }
}

async function start() {
  wirePlatformTabs();
  window.pageI18n.onChange(render);
  state.arch = await detectArchitecture();
  state.manifest = await newestManifest();
  render();
}

void start();
