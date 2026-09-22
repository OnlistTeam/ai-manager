import { createHash } from "node:crypto";
import { readFile, writeFile } from "node:fs/promises";
import { fileURLToPath } from "node:url";
import { createServer } from "vite";

import { HOUSE_PRESETS, isAllowedService } from "./provider-preset-policy.mjs";

const root = fileURLToPath(new URL("../", import.meta.url));
export const catalogFile = new URL(
  "../src-tauri/src/compat/ccswitch/provider/provider_presets.generated.json",
  import.meta.url,
);

const sourceModules = [
  {
    tool: "claude-code",
    module: "claudeProviderPresets",
    exportName: "providerPresets",
  },
  {
    tool: "codex",
    module: "codexProviderPresets",
    exportName: "codexProviderPresets",
  },
  {
    tool: "gemini-cli",
    module: "geminiProviderPresets",
    exportName: "geminiProviderPresets",
  },
  {
    tool: "opencode",
    module: "opencodeProviderPresets",
    exportName: "opencodeProviderPresets",
  },
  {
    tool: "grok-build",
    module: "grokBuildProviderPresets",
    exportName: "grokBuildProviderPresets",
  },
  {
    tool: "openclaw",
    module: "openclawProviderPresets",
    exportName: "openclawProviderPresets",
  },
  {
    tool: "hermes",
    module: "hermesProviderPresets",
    exportName: "hermesProviderPresets",
  },
  {
    tool: "pi",
    module: "piProviderPresets",
    exportName: "piProviderPresets",
  },
];

const officialPresets = [
  {
    tool: "claude-code",
    id: "official",
    serviceName: "Anthropic API",
    defaultName: "Anthropic",
    defaultModel: "claude-sonnet-5",
    websiteUrl: "https://www.anthropic.com",
    apiKeyUrl: "https://console.anthropic.com",
    official: true,
    default: true,
    settingsConfig: {
      env: {
        ANTHROPIC_BASE_URL: "https://api.anthropic.com",
        ANTHROPIC_API_KEY: "",
      },
    },
  },
  {
    tool: "codex",
    id: "official",
    serviceName: "OpenAI API",
    defaultName: "OpenAI",
    defaultModel: "gpt-5.6-sol",
    websiteUrl: "https://openai.com",
    apiKeyUrl: "https://platform.openai.com",
    official: true,
    default: true,
    settingsConfig: {
      auth: { OPENAI_API_KEY: "" },
      config:
        'model_provider = "custom"\n\n[model_providers.custom]\nname = "OpenAI"\nbase_url = "https://api.openai.com/v1"\nwire_api = "responses"\nrequires_openai_auth = true\n',
    },
  },
  {
    tool: "opencode",
    id: "official",
    serviceName: "OpenAI API",
    defaultName: "OpenAI",
    defaultModel: "gpt-5.6-sol",
    websiteUrl: "https://openai.com",
    apiKeyUrl: "https://platform.openai.com",
    official: true,
    default: true,
    settingsConfig: {
      npm: "@ai-sdk/openai",
      name: "OpenAI",
      options: {
        baseURL: "https://api.openai.com/v1",
        apiKey: "",
      },
      models: {
        "gpt-5.6-sol": { name: "gpt-5.6-sol" },
      },
    },
  },
  {
    tool: "gemini-cli",
    id: "official",
    serviceName: "Google Gemini API",
    defaultName: "Google Gemini",
    defaultModel: "gemini-3.6-flash",
    websiteUrl: "https://ai.google.dev",
    apiKeyUrl: "https://aistudio.google.com",
    official: true,
    default: true,
    settingsConfig: {
      env: {
        GOOGLE_GEMINI_BASE_URL: "https://generativelanguage.googleapis.com",
        GEMINI_API_KEY: "",
      },
      config: {},
    },
  },
];

const secretField =
  /(?:api.?key|auth.?token|access.?token|secret|password|authorization|credential)/i;
const secretAssignment =
  /(?:api_?key|auth_?token|access_?token|secret|password|authorization)\s*=\s*["']([^"']+)["']/gi;

function cloneJson(value) {
  return JSON.parse(JSON.stringify(value));
}

function safeEndpoint(value) {
  if (typeof value !== "string") return null;
  try {
    const url = new URL(value);
    if (
      url.protocol !== "https:" ||
      url.username ||
      url.password ||
      url.search ||
      url.hash
    ) {
      return null;
    }
    return url.toString().replace(/\/$/, "");
  } catch {
    return null;
  }
}

function publicOrigin(value) {
  if (typeof value !== "string") return null;
  try {
    const url = new URL(value);
    if (url.protocol !== "https:" || url.username || url.password) return null;
    return url.origin;
  } catch {
    return null;
  }
}

export function containsPlainSecret(value, parentKey = "") {
  if (typeof value === "string") {
    if (secretField.test(parentKey) && value.trim() !== "") return true;
    if (parentKey === "config") {
      secretAssignment.lastIndex = 0;
      return [...value.matchAll(secretAssignment)].some(
        (match) => match[1]?.trim() !== "",
      );
    }
    return false;
  }
  if (Array.isArray(value)) {
    return value.some((entry) => containsPlainSecret(entry, parentKey));
  }
  if (value === null || typeof value !== "object") return false;
  return Object.entries(value).some(([key, entry]) =>
    containsPlainSecret(entry, key),
  );
}

function stableId(tool, name, endpoint) {
  const slug = name
    .normalize("NFKD")
    .toLowerCase()
    .replace(/[^a-z0-9]+/g, "-")
    .replace(/^-|-$/g, "")
    .slice(0, 28);
  const digest = createHash("sha256")
    .update([tool, name, endpoint].join("\n"))
    .digest("hex")
    .slice(0, 10);
  return `${slug || "service"}-${digest}`;
}

function commonProjection(
  tool,
  preset,
  endpoint,
  defaultModel,
  settingsConfig,
) {
  const serviceName = String(preset.name ?? "").trim();
  const websiteUrl = publicOrigin(preset.websiteUrl);
  const apiKeyUrl = publicOrigin(preset.apiKeyUrl ?? preset.websiteUrl);
  if (
    !serviceName ||
    serviceName.length > 100 ||
    !websiteUrl ||
    !apiKeyUrl ||
    !defaultModel ||
    defaultModel.length > 256 ||
    containsPlainSecret(settingsConfig)
  ) {
    return null;
  }
  return {
    tool,
    id: stableId(tool, serviceName, endpoint),
    serviceName,
    defaultName: serviceName,
    defaultModel,
    websiteUrl,
    apiKeyUrl,
    official: false,
    default: false,
    settingsConfig: cloneJson(settingsConfig),
  };
}

function tomlString(value) {
  return JSON.stringify(String(value));
}

function grokNativeConfig(serviceName, endpoint, model) {
  return `[models]
default = ${tomlString(model)}

[model.${tomlString(model)}]
model = ${tomlString(model)}
base_url = ${tomlString(endpoint)}
name = ${tomlString(serviceName)}
api_key = ""
api_backend = "responses"
context_window = 500000
`;
}

function projectClaude(preset) {
  const env = preset.settingsConfig?.env;
  const endpoint = safeEndpoint(env?.ANTHROPIC_BASE_URL);
  const hasEmptyKeySlot = [
    "ANTHROPIC_AUTH_TOKEN",
    "ANTHROPIC_API_KEY",
    "OPENROUTER_API_KEY",
    "GOOGLE_API_KEY",
  ].some(
    (key) =>
      Object.hasOwn(env ?? {}, key) &&
      typeof env[key] === "string" &&
      !env[key],
  );
  if (
    preset.isOfficial ||
    preset.hidden ||
    preset.requiresOAuth ||
    preset.providerType ||
    (preset.apiFormat && preset.apiFormat !== "anthropic") ||
    !endpoint ||
    !hasEmptyKeySlot
  ) {
    return null;
  }
  const model =
    env.ANTHROPIC_MODEL ??
    env.ANTHROPIC_DEFAULT_SONNET_MODEL ??
    "claude-sonnet-5";
  return commonProjection(
    "claude-code",
    preset,
    endpoint,
    model,
    preset.settingsConfig,
  );
}

function projectCodex(preset) {
  const config = typeof preset.config === "string" ? preset.config : "";
  const endpoint = safeEndpoint(
    config.match(/(?:^|\n)base_url\s*=\s*"([^"]+)"/)?.[1],
  );
  const model = config.match(/(?:^|\n)model\s*=\s*"([^"]+)"/)?.[1];
  if (
    preset.isOfficial ||
    preset.providerType ||
    preset.auth?.OPENAI_API_KEY !== "" ||
    !endpoint ||
    !model
  ) {
    return null;
  }
  return commonProjection("codex", preset, endpoint, model, {
    auth: cloneJson(preset.auth),
    config,
  });
}

function projectGemini(preset) {
  const env = preset.settingsConfig?.env;
  const endpoint = safeEndpoint(env?.GOOGLE_GEMINI_BASE_URL);
  const hasEmptyKeySlot = ["GEMINI_API_KEY", "GOOGLE_API_KEY"].some(
    (key) =>
      Object.hasOwn(env ?? {}, key) &&
      typeof env[key] === "string" &&
      !env[key],
  );
  if (preset.category === "official" || !endpoint || !hasEmptyKeySlot) {
    return null;
  }
  const model = env.GEMINI_MODEL ?? preset.model ?? "gemini-3.6-flash";
  return commonProjection(
    "gemini-cli",
    preset,
    endpoint,
    model,
    preset.settingsConfig,
  );
}

function projectOpenCode(preset) {
  const settings = preset.settingsConfig;
  const endpoint = safeEndpoint(settings?.options?.baseURL);
  const models =
    settings?.models && typeof settings.models === "object"
      ? Object.keys(settings.models)
      : [];
  if (
    !endpoint ||
    settings?.options?.apiKey !== "" ||
    typeof settings?.npm !== "string" ||
    !settings.npm.startsWith("@ai-sdk/") ||
    models.length === 0
  ) {
    return null;
  }
  return commonProjection("opencode", preset, endpoint, models[0], settings);
}

function projectGrokBuild(preset) {
  const config = typeof preset.config === "string" ? preset.config : "";
  const endpoint = safeEndpoint(
    config.match(/(?:^|\n)base_url\s*=\s*"([^"]+)"/)?.[1],
  );
  const model = config.match(/(?:^|\n)model\s*=\s*"([^"]+)"/)?.[1];
  if (
    preset.isOfficial ||
    preset.auth?.OPENAI_API_KEY !== "" ||
    !endpoint ||
    !model
  ) {
    return null;
  }
  const projected = commonProjection("grok-build", preset, endpoint, model, {
    config: grokNativeConfig(preset.name, endpoint, model),
  });
  if (
    projected &&
    preset.name === "xAI (Grok)" &&
    endpoint === "https://api.x.ai/v1"
  ) {
    projected.official = true;
    projected.default = true;
  }
  return projected;
}

function projectOpenClaw(preset) {
  const settings = preset.settingsConfig;
  const endpoint = safeEndpoint(settings?.baseUrl);
  const models = Array.isArray(settings?.models)
    ? settings.models
        .map((model) => model?.id)
        .filter((model) => typeof model === "string" && model.trim())
    : [];
  if (
    !endpoint ||
    settings?.apiKey !== "" ||
    typeof settings?.api !== "string" ||
    models.length === 0
  ) {
    return null;
  }
  const projected = commonProjection(
    "openclaw",
    preset,
    endpoint,
    models[0],
    settings,
  );
  // Pi and OpenClaw have no first-party vendor, so their default comes
  // from HOUSE_PRESETS instead of an aggregator that happens to be listed.
  return projected;
}

function projectHermes(preset) {
  const settings = preset.settingsConfig;
  const endpoint = safeEndpoint(settings?.base_url);
  const models = Array.isArray(settings?.models)
    ? settings.models
        .map((model) => model?.id)
        .filter((model) => typeof model === "string" && model.trim())
    : [];
  if (
    !endpoint ||
    settings?.api_key !== "" ||
    typeof settings?.api_mode !== "string" ||
    models.length === 0
  ) {
    return null;
  }
  const projected = commonProjection(
    "hermes",
    preset,
    endpoint,
    models[0],
    settings,
  );
  if (projected && preset.isOfficial === true) projected.official = true;
  if (projected && preset.name === "Nous Research") projected.default = true;
  return projected;
}

function projectPi(preset) {
  const settings = preset.settingsConfig;
  const endpoint = safeEndpoint(settings?.baseUrl);
  const models = Array.isArray(settings?.models)
    ? settings.models
        .map((model) => model?.id)
        .filter((model) => typeof model === "string" && model.trim())
    : [];
  if (
    !endpoint ||
    settings?.apiKey !== "" ||
    typeof settings?.api !== "string" ||
    models.length === 0
  ) {
    return null;
  }
  const projected = commonProjection(
    "pi",
    preset,
    endpoint,
    models[0],
    settings,
  );
  // Pi and OpenClaw have no first-party vendor, so their default comes
  // from HOUSE_PRESETS instead of an aggregator that happens to be listed.
  return projected;
}

const projectors = {
  "claude-code": projectClaude,
  codex: projectCodex,
  "gemini-cli": projectGemini,
  opencode: projectOpenCode,
  "grok-build": projectGrokBuild,
  openclaw: projectOpenClaw,
  hermes: projectHermes,
  pi: projectPi,
};

async function loadSourcePresets() {
  const server = await createServer({
    root,
    configFile: false,
    appType: "custom",
    logLevel: "silent",
    server: { middlewareMode: true },
    resolve: { alias: { "@": `${root}/src` } },
  });
  try {
    return await Promise.all(
      sourceModules.map(async (source) => {
        const module = await server.ssrLoadModule(
          `/@fs/${root}/src/config/${source.module}.ts`,
        );
        return { ...source, presets: module[source.exportName] };
      }),
    );
  } finally {
    await server.close();
  }
}

/**
 * `house` is this product's own service, and it leads each tool's list.
 *
 * Position is not preselection: the dialog opens on `defaultPresetId`, which
 * stays the tool's own vendor, so leading the list costs nobody the default
 * they expect.
 */
function isHousePreset(preset) {
  return HOUSE_PRESETS.some(
    (house) => house.tool === preset.tool && house.id === preset.id,
  );
}

function comparePresets(left, right) {
  if (left.tool !== right.tool) return left.tool < right.tool ? -1 : 1;
  const leftHouse = isHousePreset(left);
  const rightHouse = isHousePreset(right);
  if (leftHouse !== rightHouse) return leftHouse ? -1 : 1;
  if (left.default !== right.default) return left.default ? -1 : 1;
  if (left.official !== right.official) return left.official ? -1 : 1;
  const leftName = left.serviceName.toLocaleLowerCase("en");
  const rightName = right.serviceName.toLocaleLowerCase("en");
  if (leftName !== rightName) return leftName < rightName ? -1 : 1;
  return left.id < right.id ? -1 : left.id > right.id ? 1 : 0;
}

export async function buildProviderPresetCatalog() {
  const sources = await loadSourcePresets();
  const projected = sources
    .flatMap(({ tool, presets }) => presets.map(projectors[tool]).filter(Boolean))
    // The upstream sources are cherry-picked, so this has to be a standing
    // filter rather than a deletion: a relay added upstream tomorrow is kept
    // out without anyone remembering to do it again.
    .filter((preset) => isAllowedService(preset.serviceName));
  const presets = [
    ...HOUSE_PRESETS.map(cloneJson),
    ...officialPresets.map(cloneJson),
    ...projected,
  ].sort(comparePresets);
  const identities = new Set();
  for (const preset of presets) {
    const identity = `${preset.tool}:${preset.id}`;
    if (identities.has(identity)) {
      throw new Error(`duplicate generated provider preset: ${identity}`);
    }
    identities.add(identity);
  }
  for (const tool of sourceModules.map((source) => source.tool)) {
    const toolPresets = presets.filter((preset) => preset.tool === tool);
    if (toolPresets.filter((preset) => preset.default).length !== 1) {
      throw new Error(`${tool} must have exactly one default provider preset`);
    }
    if (toolPresets.filter((preset) => preset.official).length > 1) {
      throw new Error(`${tool} has more than one official provider preset`);
    }
  }
  return { version: 2, presets };
}

export function serializeProviderPresetCatalog(catalog) {
  return `${JSON.stringify(catalog, null, 2)}\n`;
}

export async function writeProviderPresetCatalog() {
  const catalog = await buildProviderPresetCatalog();
  const next = serializeProviderPresetCatalog(catalog);
  const previous = await readFile(catalogFile, "utf8").catch((error) => {
    if (error?.code === "ENOENT") return "";
    throw error;
  });
  if (previous !== next) await writeFile(catalogFile, next, "utf8");
  return { changed: previous !== next, count: catalog.presets.length };
}
