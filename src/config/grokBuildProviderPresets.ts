/**
 * Preset provider configuration templates for Grok Build (Grok CLI)
 *
 * Maintained independently, with no data linkage to codexProviderPresets.ts
 * (decided by Jason on 2026-07-21). The initial entries were taken from a
 * snapshot of the Codex presets at the time; the two have since evolved
 * separately. Partner links / icons / endpoint changes need to be made
 * separately in this file.
 *
 * Inclusion rules:
 * - No official / managed OAuth preset: Grok CLI ships its own xAI
 *   subscription login, and the official state goes through the separate
 *   "Grok Official" entry (corresponding to the seed in providers_seed.rs;
 *   empty config = no custom model table written).
 * - No official direct connections for domestic models (cn_official) and no
 *   pure open-source model hosting sites (SiliconFlow / ModelScope / Novita /
 *   Nvidia / AtlasCloud): these upstreams have no Grok model and cannot be
 *   used in Grok CLI.
 * - OpenCode Go upstream has offered grok-4.5 since 2026-08, but it is still
 *   not included for now: whether the subscription gateway should include a
 *   Grok preset is a product decision that needs separate evaluation before
 *   inclusion.
 * - Only aggregators and third-party relay sites are included, with the
 *   default model uniformly set to grok-4.5; routing sites under the
 *   OpenRouter-style namespace use "x-ai/grok-4.5".
 *
 * The config field continues to use Codex-style TOML as its carrier: the
 * Grok form only extracts the three fields base_url / model / wire_api from
 * it (via the extractCodex* utilities), then rebuilds Grok CLI's own
 * config.toml.
 */
import type { ProviderCategory } from "../types";
import type { CodexApiFormat } from "../types";
import { GROK_BUILD_DEFAULT_MODEL } from "../utils/grokBuildConfig";

export interface GrokBuildProviderPreset {
  name: string;
  nameKey?: string; // i18n key for localized display name
  websiteUrl: string;
  apiKeyUrl?: string;
  auth: Record<string, string>;
  config: string; // Codex-style TOML carrier (only base_url / model / wire_api are consumed)
  isOfficial?: boolean;
  category?: ProviderCategory;
  endpointCandidates?: string[];
  icon?: string;
  iconColor?: string;
  apiFormat?: CodexApiFormat;
}

// The official entry corresponds to the backend seed ("Grok Official" in
// providers_seed.rs): empty config = no custom model table written, so Grok
// CLI falls back to its built-in xAI OAuth login.
// The preset id reuses the fixed provider id, which AddProviderDialog uses
// to drive the ensure-seed flow.
export const grokBuildOfficialPreset: GrokBuildProviderPreset = {
  name: "Grok Official",
  websiteUrl: "https://x.ai/grok",
  isOfficial: true,
  category: "official",
  auth: {},
  config: "",
  icon: "grok",
  iconColor: "currentColor",
};

/** Grok model id for routing sites under the OpenRouter-style namespace */
const OPENROUTER_STYLE_GROK_MODEL = "x-ai/grok-4.5";

const grokAuth = (): Record<string, string> => ({ OPENAI_API_KEY: "" });

function grokPresetConfig(
  providerName: string,
  baseUrl: string,
  model = GROK_BUILD_DEFAULT_MODEL,
): string {
  const tomlString = (value: string) => JSON.stringify(value);

  return `model_provider = "custom"
model = ${tomlString(model)}

[model_providers.custom]
name = ${tomlString(providerName)}
base_url = ${tomlString(baseUrl)}
wire_api = "responses"
requires_openai_auth = true`;
}

export const grokBuildProviderPresets: GrokBuildProviderPreset[] = [
  // ===== Pinned presets: file order = in-app display order =====
  {
    name: "PackyCode",
    websiteUrl: "https://www.packyapi.ai",
    apiKeyUrl: "https://www.packyapi.ai/register",
    auth: grokAuth(),
    config: grokPresetConfig("PackyCode", "https://www.packyapi.ai/v1"),
    endpointCandidates: [
      "https://www.packyapi.ai/v1",
      "https://cf.api.fan/v1",
      "https://slb-v1.api.fan/v1",
      "https://www.packyapi.com/v1",
    ],
    category: "third_party",
    icon: "packycode",
  },
  {
    name: "ZetaAPI",
    websiteUrl: "https://zetaapi.ai",
    apiKeyUrl: "https://zetaapi.ai/go/u117",
    auth: grokAuth(),
    config: grokPresetConfig("ZetaAPI", "https://api.zetaapi.ai/v1"),
    endpointCandidates: ["https://api.zetaapi.ai/v1"],
    category: "aggregator",
    icon: "zetaapi",
  },
  {
    name: "APINebula",
    websiteUrl: "https://apinebula.ai",
    apiKeyUrl: "https://apinebula.ai/VjM74M",
    auth: grokAuth(),
    config: grokPresetConfig("APINebula", "https://apinebula.ai/v1"),
    endpointCandidates: ["https://apinebula.ai/v1"],
    apiFormat: "openai_responses",
    category: "third_party",
    icon: "apinebula",
  },
  {
    name: "AICodeMirror",
    websiteUrl: "https://www.aicodemirror.ai",
    apiKeyUrl: "https://www.aicodemirror.ai/register",
    auth: grokAuth(),
    config: grokPresetConfig(
      "AICodeMirror",
      "https://api.aicodemirror.ai/api/codex/backend-api/codex",
    ),
    endpointCandidates: [
      "https://api.aicodemirror.ai/api/codex/backend-api/codex",
    ],
    icon: "aicodemirror",
    iconColor: "#000000",
  },
  {
    name: "PatewayAI",
    websiteUrl: "https://pateway.ai",
    apiKeyUrl: "https://pateway.ai/#/",
    auth: grokAuth(),
    config: grokPresetConfig("PatewayAI", "https://api.pateway.ai/v1"),
    endpointCandidates: ["https://api.pateway.ai/v1"],
    category: "third_party",
    icon: "pateway",
  },
  {
    name: "FennoAI",
    websiteUrl: "https://api.fenno.ai",
    apiKeyUrl:
      "https://api.fenno.ai/register?redirect=/purchase?tab=subscription%26group=16",
    auth: grokAuth(),
    config: grokPresetConfig("FennoAI", "https://api.fenno.ai"),
    endpointCandidates: ["https://api.fenno.ai"],
    category: "aggregator",
    icon: "fenno",
  },
  {
    name: "RunAPI",
    websiteUrl: "https://runapi.host",
    apiKeyUrl: "https://runapi.host/register",
    auth: grokAuth(),
    config: grokPresetConfig("RunAPI", "https://runapi.host/v1"),
    endpointCandidates: ["https://runapi.host/v1", "https://runapi.co/v1"],
    category: "aggregator",
    icon: "runapi",
  },
  {
    name: "Shengsuanyun",
    nameKey: "providerForm.presets.shengsuanyun",
    websiteUrl: "https://www.shengsuanyun.com/",
    apiKeyUrl: "https://www.shengsuanyun.com/",
    auth: grokAuth(),
    config: grokPresetConfig(
      "Shengsuanyun",
      "https://router.shengsuanyun.com/api/v1",
      OPENROUTER_STYLE_GROK_MODEL,
    ),
    category: "aggregator",
    icon: "shengsuanyun",
  },
  {
    name: "AIGoCode",
    websiteUrl: "https://aigocode.app",
    apiKeyUrl: "https://aigocode.app",
    auth: grokAuth(),
    config: grokPresetConfig("AIGoCode", "https://api.aigocode.app"),
    endpointCandidates: ["https://api.aigocode.app"],
    category: "third_party",
    icon: "aigocode",
    iconColor: "#5B7FFF",
  },
  {
    name: "Qiniu",
    nameKey: "providerForm.presets.qiniu",
    websiteUrl: "https://s.qiniu.com/nMvAvy",
    apiKeyUrl: "https://s.qiniu.com/nMvAvy",
    auth: grokAuth(),
    config: grokPresetConfig(
      "Qiniu",
      "https://api.qnaigc.com/bypass/openai/v1",
    ),
    endpointCandidates: [
      "https://api.qnaigc.com/bypass/openai/v1",
      "https://api.modelink.ai/bypass/openai/v1",
    ],
    category: "aggregator",
    icon: "qiniu",
  },
  {
    name: "SubRouter",
    websiteUrl: "https://subrouter.ai",
    apiKeyUrl: "https://subrouter.ai/register",
    auth: grokAuth(),
    config: grokPresetConfig("SubRouter", "https://subrouter.ai/v1"),
    endpointCandidates: ["https://subrouter.ai/v1"],
    category: "aggregator",
    icon: "subrouter",
  },
  {
    name: "APIKEY.FUN",
    websiteUrl: "https://apikey.fan",
    apiKeyUrl: "https://apikey.fan/register",
    auth: grokAuth(),
    config: grokPresetConfig("APIKEY.FUN", "https://api.apikey.fan/v1"),
    endpointCandidates: [
      "https://api.apikey.fan/v1",
      "https://api.apikey.fun/v1",
      "https://slb.apikey.fun/v1",
    ],
    apiFormat: "openai_responses",
    category: "third_party",
    icon: "apikeyfun",
  },
  {
    name: "9527CODE",
    websiteUrl: "https://9527.codes",
    apiKeyUrl: "https://9527.codes/register",
    auth: grokAuth(),
    config: grokPresetConfig("9527CODE", "https://9527.codes/v1"),
    endpointCandidates: [
      "https://9527.codes/v1",
      "https://api.9527.codes/v1",
      "https://cdn.9527.codes/v1",
    ],
    category: "aggregator",
    icon: "9527code",
  },
  {
    name: "Code0",
    websiteUrl: "https://code0.ai",
    apiKeyUrl: "https://code0.ai/agent/register/B2XHxGjGmRvqgznY",
    auth: grokAuth(),
    config: grokPresetConfig("Code0", "https://code0.ai/v1"),
    endpointCandidates: ["https://code0.ai/v1"],
    category: "aggregator",
    icon: "code0",
  },
  {
    name: "TeamoRouter",
    websiteUrl: "https://teamorouter.cn",
    apiKeyUrl: "https://teamorouter.cn/",
    auth: grokAuth(),
    config: grokPresetConfig("TeamoRouter", "https://api.teamorouter.cn/v1"),
    endpointCandidates: [
      "https://api.teamorouter.cn/v1",
      "https://api.teamorouter.com/v1",
    ],
    category: "aggregator",
    icon: "teamorouter",
  },
  {
    name: "ClaudeCN",
    websiteUrl: "https://claudecn.top",
    apiKeyUrl: "https://claudecn.ai/register",
    auth: grokAuth(),
    config: grokPresetConfig("ClaudeCN", "https://claudecn.top/v1"),
    category: "third_party",
    icon: "claudecn",
  },
  {
    name: "A6API",
    websiteUrl: "https://www.a6api.com",
    apiKeyUrl: "https://a6api.com/register",
    auth: grokAuth(),
    config: grokPresetConfig("A6API", "https://api.a6api.com/v1"),
    endpointCandidates: ["https://api.a6api.com/v1"],
    category: "aggregator",
    icon: "a6api",
  },
  {
    name: "Compshare",
    nameKey: "providerForm.presets.ucloud",
    websiteUrl: "https://www.compshare.cn",
    apiKeyUrl: "https://www.compshare.cn/coding-plan",
    auth: grokAuth(),
    config: grokPresetConfig("Compshare", "https://api.modelverse.cn/v1"),
    endpointCandidates: ["https://api.modelverse.cn/v1"],
    category: "aggregator",
    icon: "ucloud",
    iconColor: "#000000",
  },
  {
    name: "Compshare Coding Plan",
    nameKey: "providerForm.presets.ucloudCoding",
    websiteUrl: "https://www.compshare.cn",
    apiKeyUrl: "https://www.compshare.cn/coding-plan",
    auth: grokAuth(),
    config: grokPresetConfig(
      "Compshare Coding Plan",
      "https://cp.compshare.cn/v1",
    ),
    endpointCandidates: ["https://cp.compshare.cn/v1"],
    category: "aggregator",
    icon: "ucloud",
    iconColor: "#000000",
  },
  {
    name: "CCSub",
    websiteUrl: "https://www.ccsub.net",
    apiKeyUrl: "https://www.ccsub.net/register",
    auth: grokAuth(),
    config: grokPresetConfig("CCSub", "https://www.ccsub.net/v1"),
    endpointCandidates: ["https://www.ccsub.net/v1"],
    category: "aggregator",
    icon: "ccsub",
  },
  {
    name: "SSSAiCode",
    websiteUrl: "https://sssaicodeapi.com",
    apiKeyUrl: "https://sssaicodeapi.com/register",
    auth: grokAuth(),
    config: grokPresetConfig(
      "SSSAiCode",
      "https://node-hk.sssaicodeapi.com/api/v1",
    ),
    endpointCandidates: [
      "https://node-hk.sssaicodeapi.com/api/v1",
      "https://node-hk.sssaiapi.com/api/v1",
      "https://node-cf.sssaicodeapi.com/api/v1",
    ],
    category: "third_party",
    icon: "sssaicode",
    iconColor: "#000000",
  },
  {
    name: "SoleAPI",
    websiteUrl: "https://soleapi.com",
    apiKeyUrl: "https://soleapi.com",
    auth: grokAuth(),
    config: grokPresetConfig("SoleAPI", "https://soleapi.com/v1"),
    endpointCandidates: ["https://soleapi.com/v1"],
    category: "aggregator",
    icon: "soleapi",
  },
  {
    name: "Micu",
    websiteUrl: "https://www.micuapi.ai",
    apiKeyUrl: "https://www.micuapi.ai/register",
    auth: grokAuth(),
    config: grokPresetConfig("Micu", "https://www.micuapi.ai/v1"),
    endpointCandidates: ["https://www.micuapi.ai/v1"],
    category: "third_party",
    icon: "micu",
    iconColor: "#000000",
  },
  {
    name: "RightCode",
    websiteUrl: "https://www.rightapi.ai",
    apiKeyUrl: "https://www.rightapi.ai/register",
    auth: grokAuth(),
    config: grokPresetConfig("RightCode", "https://www.rightapi.ai/codex/v1"),
    category: "third_party",
    icon: "rc",
    iconColor: "#E96B2C",
  },
  {
    name: "ETok.ai",
    websiteUrl: "https://etok.ai",
    apiKeyUrl: "https://etok.ai",
    auth: grokAuth(),
    config: grokPresetConfig("ETok.ai", "https://api.etok.ai/v1"),
    endpointCandidates: ["https://api.etok.ai/v1"],
    category: "third_party",
    icon: "etok",
    iconColor: "#000000",
  },
  {
    name: "Cubence",
    websiteUrl: "https://cubence.com",
    apiKeyUrl: "https://cubence.com/signup",
    auth: grokAuth(),
    config: grokPresetConfig("Cubence", "https://api.cubence.com/v1"),
    endpointCandidates: [
      "https://api.cubence.com/v1",
      "https://api-cf.cubence.com/v1",
      "https://api-dmit.cubence.com/v1",
      "https://api-bwg.cubence.com/v1",
    ],
    category: "third_party",
    icon: "cubence",
    iconColor: "#000000",
  },
  {
    name: "CrazyRouter",
    websiteUrl: "https://www.crazyrouter.com",
    apiKeyUrl: "https://www.crazyrouter.com/register",
    auth: grokAuth(),
    config: grokPresetConfig("CrazyRouter", "https://cn.crazyrouter.com/v1"),
    endpointCandidates: ["https://cn.crazyrouter.com/v1"],
    icon: "crazyrouter",
    iconColor: "#000000",
  },
  {
    name: "DMXAPI",
    websiteUrl: "https://www.dmxapi.cn",
    auth: grokAuth(),
    config: grokPresetConfig("DMXAPI", "https://www.dmxapi.cn/v1"),
    endpointCandidates: ["https://www.dmxapi.cn/v1"],
    category: "aggregator",
  },
  {
    name: "SudoCode.chat",
    websiteUrl: "https://sudocode.chat",
    apiKeyUrl: "https://sudocode.chat/sign-up",
    auth: grokAuth(),
    config: grokPresetConfig("SudoCode.chat", "https://api.sudocode.chat/v1"),
    endpointCandidates: ["https://api.sudocode.chat/v1"],
    apiFormat: "openai_responses",
    category: "third_party",
    icon: "sudocode",
  },
  {
    name: "SudoCode.us",
    websiteUrl: "https://sudocode.us",
    apiKeyUrl: "https://sudocode.us",
    auth: grokAuth(),
    config: grokPresetConfig("SudoCode.us", "https://sudocode.us/v1"),
    endpointCandidates: ["https://sudocode.us/v1", "https://sudocode.run/v1"],
    apiFormat: "openai_responses",
    category: "third_party",
    icon: "sudocode-us",
  },
  // ===== Remaining presets: in-app display is sorted by display name, so file order here does not affect display =====
  {
    name: "xAI (Grok)",
    websiteUrl: "https://x.ai/api",
    apiKeyUrl: "https://console.x.ai",
    auth: grokAuth(),
    config: grokPresetConfig("xAI (Grok)", "https://api.x.ai/v1"),
    endpointCandidates: ["https://api.x.ai/v1"],
    apiFormat: "openai_responses",
    category: "third_party",
    icon: "xai",
    iconColor: "#000000",
  },
  {
    name: "Amux",
    websiteUrl: "https://amux.ai",
    apiKeyUrl: "https://amux.ai",
    auth: grokAuth(),
    config: grokPresetConfig("Amux", "https://api.amux.ai/v1"),
    endpointCandidates: ["https://api.amux.ai/v1"],
    category: "aggregator",
    icon: "amux",
  },
  {
    name: "AiHubMix",
    websiteUrl: "https://aihubmix.com",
    auth: grokAuth(),
    config: grokPresetConfig("AiHubMix", "https://aihubmix.com/v1"),
    endpointCandidates: [
      "https://aihubmix.com/v1",
      "https://api.aihubmix.com/v1",
    ],
    category: "aggregator",
    icon: "aihubmix",
    iconColor: "#006FFB",
  },
  {
    name: "CherryIN",
    websiteUrl: "https://open.cherryin.ai",
    apiKeyUrl: "https://open.cherryin.ai/console/token",
    auth: grokAuth(),
    config: grokPresetConfig(
      "CherryIN",
      "https://open.cherryin.net/v1",
      OPENROUTER_STYLE_GROK_MODEL,
    ),
    endpointCandidates: ["https://open.cherryin.net/v1"],
    category: "aggregator",
    icon: "cherryin",
  },
  {
    name: "RelaxyCode",
    websiteUrl: "https://www.relaxycode.com",
    apiKeyUrl: "https://www.relaxycode.com/register",
    auth: grokAuth(),
    config: grokPresetConfig("RelaxyCode", "https://www.relaxycode.com/v1"),
    category: "third_party",
    icon: "relaxcode",
  },
  {
    name: "E-FlowCode",
    websiteUrl: "https://e-flowcode.cc",
    apiKeyUrl: "https://e-flowcode.cc",
    auth: grokAuth(),
    config: grokPresetConfig("E-FlowCode", "https://e-flowcode.cc/v1"),
    endpointCandidates: ["https://e-flowcode.cc/v1"],
    category: "third_party",
    icon: "eflowcode",
    iconColor: "#000000",
  },
  {
    name: "PIPELLM",
    websiteUrl: "https://code.pipellm.ai",
    apiKeyUrl: "https://code.pipellm.ai/login",
    auth: grokAuth(),
    config: grokPresetConfig("PIPELLM", "https://cc-api.pipellm.ai/v1"),
    endpointCandidates: ["https://cc-api.pipellm.ai/v1"],
    category: "aggregator",
    icon: "pipellm",
  },
  {
    name: "OpenRouter",
    websiteUrl: "https://openrouter.ai",
    apiKeyUrl: "https://openrouter.ai/keys",
    auth: grokAuth(),
    config: grokPresetConfig(
      "OpenRouter",
      "https://openrouter.ai/api/v1",
      OPENROUTER_STYLE_GROK_MODEL,
    ),
    category: "aggregator",
    icon: "openrouter",
    iconColor: "#6566F1",
  },
  {
    name: "TheRouter",
    websiteUrl: "https://therouter.ai",
    apiKeyUrl: "https://dashboard.therouter.ai",
    auth: grokAuth(),
    config: grokPresetConfig(
      "TheRouter",
      "https://api.therouter.ai/v1",
      OPENROUTER_STYLE_GROK_MODEL,
    ),
    endpointCandidates: ["https://api.therouter.ai/v1"],
    category: "aggregator",
  },
];
