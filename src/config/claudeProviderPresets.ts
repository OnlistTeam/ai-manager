/**
 * Preset provider configuration templates
 */
import { ProviderCategory } from "../types";

export interface TemplateValueConfig {
  label: string;
  placeholder: string;
  defaultValue?: string;
  editorValue: string;
}

/**
 * Visual theme configuration for a preset provider
 */
export interface PresetTheme {
  /** Icon type: 'claude' | 'codex' | 'gemini' | 'generic' */
  icon?: "claude" | "codex" | "gemini" | "generic";
  /** Background color (selected state), supports Tailwind class names or hex colors */
  backgroundColor?: string;
  /** Text color (selected state), supports Tailwind class names or hex colors */
  textColor?: string;
}

export interface ProviderPreset {
  name: string;
  nameKey?: string; // i18n key for localized display name
  websiteUrl: string;
  // Added: link for third-party/aggregator etc. to configure API key retrieval separately
  apiKeyUrl?: string;
  settingsConfig: object;
  isOfficial?: boolean; // Marks whether this is an official preset
  category?: ProviderCategory; // Added: category
  // Added: specifies the API key field name used by this preset (default: ANTHROPIC_AUTH_TOKEN)
  apiKeyField?: "ANTHROPIC_AUTH_TOKEN" | "ANTHROPIC_API_KEY";
  // Added: template variable definitions, used to dynamically substitute values in the config
  templateValues?: Record<string, TemplateValueConfig>; // editorValue stores the live input value in the editor
  // Added: candidate request address list (for address management/speed testing)
  endpointCandidates?: string[];
  // Added: visual theme configuration
  theme?: PresetTheme;
  // Icon configuration
  icon?: string; // Icon name
  iconColor?: string; // Icon color

  // Claude API format (used only by Claude providers)
  // - "anthropic" (default): Anthropic Messages API format, passed through directly
  // - "openai_chat": OpenAI Chat Completions format, requires format conversion
  // - "openai_responses": OpenAI Responses API format, requires format conversion
  // - "gemini_native": Gemini Native generateContent API format, requires format conversion
  apiFormat?:
    | "anthropic"
    | "openai_chat"
    | "openai_responses"
    | "gemini_native";

  // Provider type identifier (used for special provider detection)
  // - "github_copilot": GitHub Copilot provider (requires OAuth authentication)
  // - "codex_oauth": OpenAI Codex via ChatGPT Plus/Pro reverse proxy (requires OAuth authentication)
  providerType?: "github_copilot" | "codex_oauth" | "xai_oauth";

  // Whether OAuth authentication is required (instead of an API key)
  requiresOAuth?: boolean;

  // Whether to hide this preset in the UI (the preset still exists, just not shown in the list)
  hidden?: boolean;

  // Full URL used to fetch the model list (overrides the automatic candidate logic)
  // When absent, the backend automatically tries /v1/models, /models, and variants with known compatibility sub-paths stripped, based on baseURL.
  modelsUrl?: string;
}

export const providerPresets: ProviderPreset[] = [
  {
    name: "Claude Official",
    websiteUrl: "https://www.anthropic.com/claude-code",
    settingsConfig: {
      env: {},
    },
    isOfficial: true, // Explicitly marks this as the official preset
    category: "official",
    theme: {
      icon: "claude",
      backgroundColor: "#D97757",
      textColor: "#FFFFFF",
    },
    icon: "anthropic",
    iconColor: "#D4915D",
  },
  // ===== Pinned presets: file order = in-app display order =====
  {
    name: "Kimi",
    websiteUrl: "https://platform.kimi.com",
    settingsConfig: {
      env: {
        ANTHROPIC_BASE_URL: "https://api.moonshot.cn/anthropic",
        ANTHROPIC_AUTH_TOKEN: "",
        ANTHROPIC_MODEL: "kimi-k2.7-code",
        ANTHROPIC_DEFAULT_HAIKU_MODEL: "kimi-k2.7-code",
        ANTHROPIC_DEFAULT_SONNET_MODEL: "kimi-k2.7-code",
        ANTHROPIC_DEFAULT_OPUS_MODEL: "kimi-k2.7-code",
      },
    },
    category: "cn_official",
    icon: "kimi",
    iconColor: "#6366F1",
  },
  {
    name: "Kimi For Coding",
    websiteUrl: "https://www.kimi.com/code/",
    settingsConfig: {
      env: {
        ANTHROPIC_BASE_URL: "https://api.kimi.com/coding/",
        ANTHROPIC_AUTH_TOKEN: "",
        // CLAUDE_CODE_MAX_CONTEXT_TOKENS only takes effect for non-claude--prefixed model ids,
        // so the endpoint alias kimi-for-coding must be routed explicitly (consistent with the codex/hermes/opencode presets)
        ANTHROPIC_MODEL: "kimi-for-coding",
        ANTHROPIC_DEFAULT_HAIKU_MODEL: "kimi-for-coding",
        ANTHROPIC_DEFAULT_SONNET_MODEL: "kimi-for-coding",
        ANTHROPIC_DEFAULT_OPUS_MODEL: "kimi-for-coding",
        // Pin both keys to 256K: the compaction window = min(model window, value), so setting it
        // equal to the window behaves the same as not setting it, but pinning it explicitly blocks
        // a smaller compaction point pushed by a remote experiment; adjust by editing the JSON directly, no form field is exposed
        CLAUDE_CODE_MAX_CONTEXT_TOKENS: "262144",
        CLAUDE_CODE_AUTO_COMPACT_WINDOW: "262144",
      },
    },
    category: "cn_official",
    icon: "kimi",
    iconColor: "#6366F1",
  },
  {
    name: "PackyCode",
    websiteUrl: "https://www.packyapi.ai",
    apiKeyUrl: "https://www.packyapi.ai/register",
    settingsConfig: {
      env: {
        ANTHROPIC_BASE_URL: "https://www.packyapi.ai",
        ANTHROPIC_AUTH_TOKEN: "",
      },
    },
    // Candidate request addresses (for address management/speed testing)
    endpointCandidates: [
      "https://www.packyapi.ai",
      "https://cf.api.fan",
      "https://slb-v1.api.fan",
      "https://www.packyapi.com",
    ],
    category: "third_party",
    icon: "packycode",
  },
  {
    name: "ZetaAPI",
    websiteUrl: "https://zetaapi.ai",
    apiKeyUrl: "https://zetaapi.ai/go/u117",
    settingsConfig: {
      env: {
        ANTHROPIC_BASE_URL: "https://api.zetaapi.ai",
        ANTHROPIC_AUTH_TOKEN: "",
      },
    },
    category: "aggregator",
    icon: "zetaapi",
  },
  {
    name: "APINebula",
    websiteUrl: "https://apinebula.ai",
    apiKeyUrl: "https://apinebula.ai/VjM74M",
    settingsConfig: {
      env: {
        ANTHROPIC_BASE_URL: "https://apinebula.ai",
        ANTHROPIC_AUTH_TOKEN: "",
        CLAUDE_CODE_DISABLE_NONESSENTIAL_TRAFFIC: "1",
      },
    },
    endpointCandidates: ["https://apinebula.ai"],
    category: "third_party",
    icon: "apinebula",
  },
  {
    name: "AICodeMirror",
    websiteUrl: "https://www.aicodemirror.ai",
    apiKeyUrl: "https://www.aicodemirror.ai/register",
    settingsConfig: {
      env: {
        ANTHROPIC_BASE_URL: "https://api.aicodemirror.ai/api/claudecode",
        ANTHROPIC_AUTH_TOKEN: "",
      },
    },
    endpointCandidates: ["https://api.aicodemirror.ai/api/claudecode"],
    category: "third_party",
    icon: "aicodemirror",
    iconColor: "#000000",
  },
  {
    name: "PatewayAI",
    websiteUrl: "https://pateway.ai",
    apiKeyUrl: "https://pateway.ai/#/",
    apiKeyField: "ANTHROPIC_API_KEY",
    settingsConfig: {
      env: {
        ANTHROPIC_BASE_URL: "https://api.pateway.ai",
        ANTHROPIC_API_KEY: "",
      },
    },
    category: "third_party",
    icon: "pateway",
  },
  {
    name: "FennoAI",
    websiteUrl: "https://api.fenno.ai",
    apiKeyUrl:
      "https://api.fenno.ai/register?redirect=/purchase?tab=subscription%26group=16",
    settingsConfig: {
      env: {
        ANTHROPIC_BASE_URL: "https://api.fenno.ai",
        ANTHROPIC_AUTH_TOKEN: "",
      },
    },
    category: "aggregator",
    icon: "fenno",
  },
  {
    name: "RunAPI",
    websiteUrl: "https://runapi.host",
    apiKeyUrl: "https://runapi.host/register",
    settingsConfig: {
      env: {
        ANTHROPIC_BASE_URL: "https://runapi.host",
        ANTHROPIC_AUTH_TOKEN: "",
      },
    },
    endpointCandidates: ["https://runapi.host", "https://runapi.co"],
    category: "aggregator",
    icon: "runapi",
  },
  {
    name: "Shengsuanyun",
    nameKey: "providerForm.presets.shengsuanyun",
    websiteUrl: "https://www.shengsuanyun.com/",
    apiKeyUrl: "https://www.shengsuanyun.com/",
    settingsConfig: {
      env: {
        ANTHROPIC_BASE_URL: "https://router.shengsuanyun.com/api",
        ANTHROPIC_AUTH_TOKEN: "",
        ANTHROPIC_MODEL: "anthropic/claude-sonnet-5",
        ANTHROPIC_DEFAULT_HAIKU_MODEL: "anthropic/claude-haiku-4.5",
        ANTHROPIC_DEFAULT_SONNET_MODEL: "anthropic/claude-sonnet-5",
        ANTHROPIC_DEFAULT_OPUS_MODEL: "anthropic/claude-opus-5",
      },
    },
    category: "aggregator",
    icon: "shengsuanyun",
  },
  {
    name: "AIGoCode",
    websiteUrl: "https://aigocode.app",
    apiKeyUrl: "https://aigocode.app",
    settingsConfig: {
      env: {
        ANTHROPIC_BASE_URL: "https://api.aigocode.app",
        ANTHROPIC_AUTH_TOKEN: "",
      },
    },
    // Candidate request addresses (for address management/speed testing)
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
    settingsConfig: {
      env: {
        ANTHROPIC_BASE_URL: "https://api.qnaigc.com",
        ANTHROPIC_AUTH_TOKEN: "",
      },
    },
    endpointCandidates: ["https://api.qnaigc.com", "https://api.modelink.ai"],
    category: "aggregator",
    icon: "qiniu",
  },
  {
    name: "AICoding",
    websiteUrl: "https://aicoding.inc",
    apiKeyUrl: "https://aicoding.inc",
    settingsConfig: {
      env: {
        ANTHROPIC_BASE_URL: "https://api.aicoding.inc",
        ANTHROPIC_AUTH_TOKEN: "",
      },
    },
    endpointCandidates: ["https://api.aicoding.inc"],
    category: "third_party",
    icon: "aicoding",
    iconColor: "#000000",
  },
  {
    name: "SubRouter",
    websiteUrl: "https://subrouter.ai",
    apiKeyUrl: "https://subrouter.ai/register",
    settingsConfig: {
      env: {
        ANTHROPIC_BASE_URL: "https://subrouter.ai",
        ANTHROPIC_AUTH_TOKEN: "",
      },
    },
    category: "aggregator",
    icon: "subrouter",
  },
  {
    name: "APIKEY.FUN",
    websiteUrl: "https://apikey.fan",
    apiKeyUrl: "https://apikey.fan/register",
    settingsConfig: {
      env: {
        ANTHROPIC_BASE_URL: "https://api.apikey.fan",
        ANTHROPIC_AUTH_TOKEN: "",
        CLAUDE_CODE_DISABLE_NONESSENTIAL_TRAFFIC: "1",
      },
    },
    endpointCandidates: [
      "https://api.apikey.fan",
      "https://api.apikey.fun",
      "https://slb.apikey.fun",
    ],
    category: "third_party",
    icon: "apikeyfun",
  },
  {
    name: "9527CODE",
    websiteUrl: "https://9527.codes",
    apiKeyUrl: "https://9527.codes/register",
    settingsConfig: {
      env: {
        ANTHROPIC_BASE_URL: "https://9527.codes",
        ANTHROPIC_AUTH_TOKEN: "",
      },
    },
    endpointCandidates: [
      "https://9527.codes",
      "https://api.9527.codes",
      "https://cdn.9527.codes",
    ],
    category: "aggregator",
    icon: "9527code",
  },
  {
    name: "ClaudeAPI",
    websiteUrl: "https://www.apito.ai",
    apiKeyUrl: "https://console.apito.ai/agent/register/pQBql2buaqiX3dDS",
    settingsConfig: {
      env: {
        ANTHROPIC_BASE_URL: "https://gw.apito.ai",
        ANTHROPIC_AUTH_TOKEN: "",
      },
    },
    category: "aggregator",
    icon: "claudeapi",
  },
  {
    name: "Code0",
    websiteUrl: "https://code0.ai",
    apiKeyUrl: "https://code0.ai/agent/register/B2XHxGjGmRvqgznY",
    settingsConfig: {
      env: {
        ANTHROPIC_BASE_URL: "https://code0.ai",
        ANTHROPIC_AUTH_TOKEN: "",
      },
    },
    category: "aggregator",
    icon: "code0",
  },
  {
    name: "TeamoRouter",
    websiteUrl: "https://teamorouter.cn",
    apiKeyUrl: "https://teamorouter.cn/",
    settingsConfig: {
      env: {
        ANTHROPIC_BASE_URL: "https://api.teamorouter.cn",
        ANTHROPIC_AUTH_TOKEN: "",
      },
    },
    category: "aggregator",
    endpointCandidates: [
      "https://api.teamorouter.cn",
      "https://api.teamorouter.com",
    ],
    icon: "teamorouter",
  },
  {
    name: "PPIO",
    websiteUrl: "https://ppio.com",
    apiKeyUrl: "https://ppio.com",
    settingsConfig: {
      env: {
        ANTHROPIC_BASE_URL: "https://api.ppio.com/anthropic",
        ANTHROPIC_AUTH_TOKEN: "",
        ANTHROPIC_MODEL: "deepseek/deepseek-v4-flash-0731",
        ANTHROPIC_DEFAULT_HAIKU_MODEL: "deepseek/deepseek-v4-flash-0731",
        ANTHROPIC_DEFAULT_SONNET_MODEL: "deepseek/deepseek-v4-flash-0731",
        ANTHROPIC_DEFAULT_OPUS_MODEL: "deepseek/deepseek-v4-flash-0731",
      },
    },
    category: "aggregator",
    endpointCandidates: ["https://api.ppio.com/anthropic"],
    modelsUrl: "https://api.ppio.com/openai/v1/models",
    icon: "ppio",
    iconColor: "#2874FF",
  },
  {
    name: "ClaudeCN",
    websiteUrl: "https://claudecn.top",
    apiKeyUrl: "https://claudecn.ai/register",
    settingsConfig: {
      env: {
        ANTHROPIC_BASE_URL: "https://claudecn.top",
        ANTHROPIC_AUTH_TOKEN: "",
      },
    },
    category: "third_party",
    icon: "claudecn",
  },
  {
    name: "火山 Agent Plan",
    websiteUrl: "https://www.volcengine.com/activity/agentplan",
    apiKeyUrl: "https://www.volcengine.com/activity/agentplan",
    settingsConfig: {
      env: {
        ANTHROPIC_BASE_URL: "https://ark.cn-beijing.volces.com/api/plan",
        ANTHROPIC_AUTH_TOKEN: "",
        ANTHROPIC_MODEL: "ark-code-latest",
        ANTHROPIC_DEFAULT_HAIKU_MODEL: "ark-code-latest",
        ANTHROPIC_DEFAULT_SONNET_MODEL: "ark-code-latest",
        ANTHROPIC_DEFAULT_OPUS_MODEL: "ark-code-latest",
      },
    },
    category: "cn_official",
    icon: "huoshan",
    iconColor: "#3370FF",
  },
  {
    name: "火山 Coding Plan",
    websiteUrl: "https://www.volcengine.com/activity/codingplan",
    apiKeyUrl: "https://www.volcengine.com/activity/codingplan",
    settingsConfig: {
      env: {
        ANTHROPIC_BASE_URL: "https://ark.cn-beijing.volces.com/api/coding",
        ANTHROPIC_AUTH_TOKEN: "",
        ANTHROPIC_MODEL: "ark-code-latest",
        ANTHROPIC_DEFAULT_HAIKU_MODEL: "ark-code-latest",
        ANTHROPIC_DEFAULT_SONNET_MODEL: "ark-code-latest",
        ANTHROPIC_DEFAULT_OPUS_MODEL: "ark-code-latest",
      },
    },
    category: "cn_official",
    icon: "huoshan",
    iconColor: "#3370FF",
  },
  {
    name: "BytePlus",
    websiteUrl: "https://www.byteplus.com/en/product/modelark",
    apiKeyUrl: "https://www.byteplus.com/en/product/modelark",
    settingsConfig: {
      env: {
        ANTHROPIC_BASE_URL:
          "https://ark.ap-southeast.bytepluses.com/api/coding",
        ANTHROPIC_AUTH_TOKEN: "",
        ANTHROPIC_MODEL: "ark-code-latest",
        ANTHROPIC_DEFAULT_HAIKU_MODEL: "ark-code-latest",
        ANTHROPIC_DEFAULT_SONNET_MODEL: "ark-code-latest",
        ANTHROPIC_DEFAULT_OPUS_MODEL: "ark-code-latest",
      },
    },
    category: "cn_official",
    icon: "byteplus",
    iconColor: "#3370FF",
  },
  {
    name: "Volcengine Doubao",
    nameKey: "providerForm.presets.doubaoseed",
    websiteUrl:
      "https://console.volcengine.com/ark/region:ark+cn-beijing/apiKey?apikey=%7B%7D",
    apiKeyUrl:
      "https://console.volcengine.com/ark/region:ark+cn-beijing/apiKey?apikey=%7B%7D",
    settingsConfig: {
      env: {
        ANTHROPIC_BASE_URL: "https://ark.cn-beijing.volces.com/api/compatible",
        ANTHROPIC_AUTH_TOKEN: "",
        API_TIMEOUT_MS: "3000000",
        ANTHROPIC_MODEL: "doubao-seed-2-1-pro-260628",
        ANTHROPIC_DEFAULT_SONNET_MODEL: "doubao-seed-2-1-pro-260628",
        ANTHROPIC_DEFAULT_OPUS_MODEL: "doubao-seed-2-1-pro-260628",
        ANTHROPIC_DEFAULT_HAIKU_MODEL: "doubao-seed-2-1-pro-260628",
      },
    },
    category: "cn_official",
    icon: "doubao",
    iconColor: "#3370FF",
  },
  {
    name: "SiliconFlow",
    websiteUrl: "https://siliconflow.cn",
    apiKeyUrl: "https://cloud.siliconflow.cn",
    settingsConfig: {
      env: {
        ANTHROPIC_BASE_URL: "https://api.siliconflow.cn",
        ANTHROPIC_AUTH_TOKEN: "",
        ANTHROPIC_MODEL: "Pro/MiniMaxAI/MiniMax-M2.5",
        ANTHROPIC_DEFAULT_HAIKU_MODEL: "Pro/MiniMaxAI/MiniMax-M2.5",
        ANTHROPIC_DEFAULT_SONNET_MODEL: "Pro/MiniMaxAI/MiniMax-M2.5",
        ANTHROPIC_DEFAULT_OPUS_MODEL: "Pro/MiniMaxAI/MiniMax-M2.5",
      },
    },
    category: "aggregator",
    icon: "siliconflow",
    iconColor: "#6E29F6",
  },
  {
    name: "SiliconFlow en",
    websiteUrl: "https://siliconflow.com",
    apiKeyUrl: "https://cloud.siliconflow.cn",
    settingsConfig: {
      env: {
        ANTHROPIC_BASE_URL: "https://api.siliconflow.com",
        ANTHROPIC_AUTH_TOKEN: "",
        ANTHROPIC_MODEL: "MiniMaxAI/MiniMax-M3",
        ANTHROPIC_DEFAULT_HAIKU_MODEL: "MiniMaxAI/MiniMax-M3",
        ANTHROPIC_DEFAULT_SONNET_MODEL: "MiniMaxAI/MiniMax-M3",
        ANTHROPIC_DEFAULT_OPUS_MODEL: "MiniMaxAI/MiniMax-M3",
      },
    },
    category: "aggregator",
    icon: "siliconflow",
    iconColor: "#000000",
  },
  {
    name: "A6API",
    websiteUrl: "https://www.a6api.com",
    apiKeyUrl: "https://a6api.com/register",
    settingsConfig: {
      env: {
        ANTHROPIC_BASE_URL: "https://api.a6api.com",
        ANTHROPIC_AUTH_TOKEN: "",
      },
    },
    category: "aggregator",
    icon: "a6api",
  },
  {
    name: "Compshare",
    nameKey: "providerForm.presets.ucloud",
    websiteUrl: "https://www.compshare.cn",
    apiKeyUrl: "https://www.compshare.cn/coding-plan",
    settingsConfig: {
      env: {
        ANTHROPIC_BASE_URL: "https://api.modelverse.cn",
        ANTHROPIC_AUTH_TOKEN: "",
      },
    },
    endpointCandidates: ["https://api.modelverse.cn"],
    category: "aggregator",
    icon: "ucloud",
    iconColor: "#000000",
  },
  {
    name: "Compshare Coding Plan",
    nameKey: "providerForm.presets.ucloudCoding",
    websiteUrl: "https://www.compshare.cn",
    apiKeyUrl: "https://www.compshare.cn/coding-plan",
    settingsConfig: {
      env: {
        ANTHROPIC_BASE_URL: "https://cp.compshare.cn",
        ANTHROPIC_AUTH_TOKEN: "",
      },
    },
    endpointCandidates: ["https://cp.compshare.cn"],
    category: "aggregator",
    icon: "ucloud",
    iconColor: "#000000",
  },
  {
    name: "CCSub",
    websiteUrl: "https://www.ccsub.net",
    apiKeyUrl: "https://www.ccsub.net/register",
    settingsConfig: {
      env: {
        ANTHROPIC_BASE_URL: "https://www.ccsub.net",
        ANTHROPIC_AUTH_TOKEN: "",
      },
    },
    category: "aggregator",
    icon: "ccsub",
  },
  {
    name: "SSSAiCode",
    websiteUrl: "https://sssaicodeapi.com",
    apiKeyUrl: "https://sssaicodeapi.com/register",
    settingsConfig: {
      env: {
        ANTHROPIC_BASE_URL: "https://node-hk.sssaicodeapi.com/api",
        ANTHROPIC_AUTH_TOKEN: "",
      },
    },
    endpointCandidates: [
      "https://node-hk.sssaicodeapi.com/api",
      "https://node-hk.sssaiapi.com/api",
      "https://node-cf.sssaicodeapi.com/api",
    ],
    category: "third_party",
    icon: "sssaicode",
    iconColor: "#000000",
  },
  {
    name: "SoleAPI",
    websiteUrl: "https://soleapi.com",
    apiKeyUrl: "https://soleapi.com",
    settingsConfig: {
      env: {
        ANTHROPIC_BASE_URL: "https://soleapi.com",
        ANTHROPIC_AUTH_TOKEN: "",
      },
    },
    endpointCandidates: ["https://soleapi.com"],
    category: "aggregator",
    icon: "soleapi",
  },
  {
    name: "Micu",
    websiteUrl: "https://www.micuapi.ai",
    apiKeyUrl: "https://www.micuapi.ai/register",
    settingsConfig: {
      env: {
        ANTHROPIC_BASE_URL: "https://www.micuapi.ai",
        ANTHROPIC_AUTH_TOKEN: "",
      },
    },
    endpointCandidates: ["https://www.micuapi.ai"],
    category: "third_party",
    icon: "micu",
    iconColor: "#000000",
  },
  {
    name: "RightCode",
    websiteUrl: "https://www.rightapi.ai",
    apiKeyUrl: "https://www.rightapi.ai/register",
    settingsConfig: {
      env: {
        ANTHROPIC_BASE_URL: "https://www.rightapi.ai/claude",
        ANTHROPIC_AUTH_TOKEN: "",
      },
    },
    category: "third_party",
    icon: "rc",
    iconColor: "#E96B2C",
  },
  {
    name: "ETok.ai",
    websiteUrl: "https://etok.ai",
    apiKeyUrl: "https://etok.ai",
    settingsConfig: {
      env: {
        ANTHROPIC_BASE_URL: "https://api.etok.ai",
        ANTHROPIC_AUTH_TOKEN: "",
      },
    },
    category: "third_party",
    icon: "etok",
    iconColor: "#000000",
  },
  {
    name: "Cubence",
    websiteUrl: "https://cubence.com",
    apiKeyUrl: "https://cubence.com/signup",
    settingsConfig: {
      env: {
        ANTHROPIC_BASE_URL: "https://api.cubence.com",
        ANTHROPIC_AUTH_TOKEN: "",
      },
    },
    endpointCandidates: [
      "https://api.cubence.com",
      "https://api-cf.cubence.com",
      "https://api-dmit.cubence.com",
      "https://api-bwg.cubence.com",
    ],
    category: "third_party",
    icon: "cubence",
    iconColor: "#000000",
  },
  {
    name: "CrazyRouter",
    websiteUrl: "https://www.crazyrouter.com",
    apiKeyUrl: "https://www.crazyrouter.com/register",
    settingsConfig: {
      env: {
        ANTHROPIC_BASE_URL: "https://cn.crazyrouter.com",
        ANTHROPIC_AUTH_TOKEN: "",
      },
    },
    endpointCandidates: ["https://cn.crazyrouter.com"],
    category: "third_party",
    icon: "crazyrouter",
    iconColor: "#000000",
  },
  {
    name: "DMXAPI",
    websiteUrl: "https://www.dmxapi.cn",
    apiKeyUrl: "https://www.dmxapi.cn",
    settingsConfig: {
      env: {
        ANTHROPIC_BASE_URL: "https://www.dmxapi.cn",
        ANTHROPIC_AUTH_TOKEN: "",
      },
    },
    // Candidate request addresses (for address management/speed testing); users can choose/override
    endpointCandidates: ["https://www.dmxapi.cn", "https://api.dmxapi.cn"],
    category: "aggregator",
  },
  {
    name: "SudoCode.chat",
    websiteUrl: "https://sudocode.chat",
    apiKeyUrl: "https://sudocode.chat/sign-up",
    settingsConfig: {
      env: {
        ANTHROPIC_BASE_URL: "https://api.sudocode.chat",
        ANTHROPIC_AUTH_TOKEN: "",
        API_TIMEOUT_MS: "300000",
      },
    },
    endpointCandidates: ["https://api.sudocode.chat"],
    category: "third_party",
    icon: "sudocode",
  },
  {
    name: "SudoCode.us",
    websiteUrl: "https://sudocode.us",
    apiKeyUrl: "https://sudocode.us",
    settingsConfig: {
      env: {
        ANTHROPIC_BASE_URL: "https://sudocode.us",
        ANTHROPIC_AUTH_TOKEN: "",
        API_TIMEOUT_MS: "300000",
      },
    },
    endpointCandidates: ["https://sudocode.us", "https://sudocode.run"],
    category: "third_party",
    icon: "sudocode-us",
  },
  {
    name: "XycAi",
    websiteUrl: "https://xycai.us",
    apiKeyUrl: "https://xycai.us/register",
    // Note: this provider uses ANTHROPIC_API_KEY (rather than ANTHROPIC_AUTH_TOKEN)
    apiKeyField: "ANTHROPIC_API_KEY",
    settingsConfig: {
      env: {
        ANTHROPIC_BASE_URL: "https://apicdn.xycai.us",
        ANTHROPIC_API_KEY: "",
      },
    },
    endpointCandidates: ["https://apicdn.xycai.us", "https://apicdn.xyc.ai"],
    category: "aggregator",
    icon: "xycai",
  },
  // ===== Remaining presets: in-app display is sorted by display name, so file order here does not affect display =====
  {
    name: "Amux",
    websiteUrl: "https://amux.ai",
    apiKeyUrl: "https://amux.ai",
    settingsConfig: {
      env: {
        ANTHROPIC_BASE_URL: "https://api.amux.ai",
        ANTHROPIC_AUTH_TOKEN: "",
      },
    },
    category: "aggregator",
    icon: "amux",
  },
  {
    name: "AtlasCloud",
    websiteUrl: "https://www.atlascloud.ai/console/coding-plan",
    apiKeyUrl: "https://www.atlascloud.ai/console/coding-plan",
    settingsConfig: {
      env: {
        ANTHROPIC_BASE_URL: "https://api.atlascloud.ai",
        ANTHROPIC_AUTH_TOKEN: "",
        ANTHROPIC_MODEL: "zai-org/glm-5.1",
        ANTHROPIC_DEFAULT_HAIKU_MODEL: "zai-org/glm-5.1",
        ANTHROPIC_DEFAULT_SONNET_MODEL: "zai-org/glm-5.1",
        ANTHROPIC_DEFAULT_OPUS_MODEL: "zai-org/glm-5.1",
        CLAUDE_CODE_DISABLE_EXPERIMENTAL_BETAS: "1",
      },
    },
    endpointCandidates: ["https://api.atlascloud.ai"],
    category: "aggregator",
    icon: "atlascloud",
  },
  {
    name: "Gemini Native",
    websiteUrl: "https://ai.google.dev/gemini-api",
    apiKeyUrl: "https://aistudio.google.com/app/apikey",
    apiKeyField: "ANTHROPIC_API_KEY",
    settingsConfig: {
      env: {
        ANTHROPIC_BASE_URL: "https://generativelanguage.googleapis.com",
        ANTHROPIC_API_KEY: "",
        ANTHROPIC_MODEL: "gemini-3.6-flash",
        ANTHROPIC_DEFAULT_HAIKU_MODEL: "gemini-3.6-flash",
        ANTHROPIC_DEFAULT_SONNET_MODEL: "gemini-3.6-flash",
        ANTHROPIC_DEFAULT_OPUS_MODEL: "gemini-3.6-flash",
      },
    },
    category: "third_party",
    apiFormat: "gemini_native",
    endpointCandidates: ["https://generativelanguage.googleapis.com"],
    icon: "gemini",
    iconColor: "#4285F4",
  },
  {
    name: "DeepSeek",
    websiteUrl: "https://platform.deepseek.com",
    settingsConfig: {
      env: {
        ANTHROPIC_BASE_URL: "https://api.deepseek.com/anthropic",
        ANTHROPIC_AUTH_TOKEN: "",
        ANTHROPIC_MODEL: "deepseek-v4-pro",
        ANTHROPIC_DEFAULT_HAIKU_MODEL: "deepseek-v4-flash",
        ANTHROPIC_DEFAULT_SONNET_MODEL: "deepseek-v4-pro",
        ANTHROPIC_DEFAULT_OPUS_MODEL: "deepseek-v4-pro",
      },
    },
    category: "cn_official",
    // The Anthropic compatibility layer is mounted at the /anthropic sub-path; /models is a separate endpoint at the root
    modelsUrl: "https://api.deepseek.com/models",
    icon: "deepseek",
    iconColor: "#1E88E5",
  },
  {
    name: "OpenCode Go",
    websiteUrl: "https://opencode.ai/go",
    apiKeyUrl: "https://opencode.ai/go",
    // The Go gateway's /v1/messages only recognizes x-api-key (Bearer is silently ignored),
    // so ANTHROPIC_API_KEY must be used and not switched back to ANTHROPIC_AUTH_TOKEN.
    // Direct connections to the Anthropic endpoint work for every Go model except grok-4.5;
    // Chat-group models (DeepSeek/GLM/Kimi, etc.) rely on server-side format conversion at the gateway (not documented as guaranteed).
    apiKeyField: "ANTHROPIC_API_KEY",
    settingsConfig: {
      env: {
        ANTHROPIC_BASE_URL: "https://opencode.ai/zen/go",
        ANTHROPIC_API_KEY: "",
        ANTHROPIC_MODEL: "deepseek-v4-flash",
        ANTHROPIC_DEFAULT_HAIKU_MODEL: "deepseek-v4-flash",
        ANTHROPIC_DEFAULT_SONNET_MODEL: "deepseek-v4-flash",
        ANTHROPIC_DEFAULT_OPUS_MODEL: "deepseek-v4-flash",
      },
    },
    category: "third_party",
    endpointCandidates: ["https://opencode.ai/zen/go"],
    icon: "opencode",
    iconColor: "#211E1E",
  },
  {
    // Tencent Cloud Token Plan Personal Edition (1823/130060, 2026-08-21 edition): the General
    // and Hy series share the same endpoint and API key; Auto smart routing's call ID is tc-code-latest.
    // Note this is a different product line from the TokenHub pay-as-you-go API marketplace (1823
    // line, e.g. the /v1 endpoint of the Hunyuan preset); the subscription key only works with the /plan endpoint
    name: "Tencent Token Plan",
    websiteUrl: "https://cloud.tencent.com/product/tokenhub",
    apiKeyUrl: "https://console.cloud.tencent.com/tokenhub/tokenplan",
    settingsConfig: {
      env: {
        ANTHROPIC_BASE_URL:
          "https://api.lkeap.cloud.tencent.com/plan/anthropic",
        ANTHROPIC_AUTH_TOKEN: "",
        ANTHROPIC_MODEL: "tc-code-latest",
        ANTHROPIC_DEFAULT_HAIKU_MODEL: "tc-code-latest",
        ANTHROPIC_DEFAULT_SONNET_MODEL: "tc-code-latest",
        ANTHROPIC_DEFAULT_OPUS_MODEL: "tc-code-latest",
      },
    },
    category: "cn_official",
    endpointCandidates: ["https://api.lkeap.cloud.tencent.com/plan/anthropic"],
    // /plan/v3/models has been verified working with a real key (2026-08-31, returns 19 models
    // in the plan including aliases); the enterprise/international endpoints have no /models (404), so only this preset overrides it
    modelsUrl: "https://api.lkeap.cloud.tencent.com/plan/v3/models",
    icon: "tencent",
    iconColor: "#0052D9",
  },
  {
    // International site (Singapore region) Personal Edition (intl 1300/81315, 2026-08-20 edition):
    // Auto's call ID is auto (unlike the domestic Personal Edition's tc-code-latest), and the model
    // lineup also differs from the domestic one (no GLM-5/5.1/Hy3, but has GLM-5.2/MiniMax-M3). The
    // endpoint uses the tencentcloudmaas.com domain designated by the international site's
    // documentation (DNS resolution has been verified to point to a Singapore node); the domestic
    // site's documentation gives tokenhub-intl.tencentmaas.com for the Singapore region -- keys are
    // independent per site and not interchangeable, so neither is a candidate for the other
    name: "Tencent Token Plan (Intl)",
    websiteUrl: "https://www.tencentcloud.com/products/tokenhub",
    apiKeyUrl: "https://console.tencentcloud.com/tokenhub/tokenplan",
    settingsConfig: {
      env: {
        ANTHROPIC_BASE_URL:
          "https://tokenhub-intl.tencentcloudmaas.com/plan/anthropic",
        ANTHROPIC_AUTH_TOKEN: "",
        ANTHROPIC_MODEL: "auto",
        ANTHROPIC_DEFAULT_HAIKU_MODEL: "auto",
        ANTHROPIC_DEFAULT_SONNET_MODEL: "auto",
        ANTHROPIC_DEFAULT_OPUS_MODEL: "auto",
      },
    },
    category: "cn_official",
    endpointCandidates: [
      "https://tokenhub-intl.tencentcloudmaas.com/plan/anthropic",
    ],
    icon: "tencent",
    iconColor: "#0052D9",
  },
  {
    // Token Plan Enterprise Pro package (1823/130659, 2026-08-25 edition): the Guangzhou region
    // endpoint is tencentmaas.com as designated by the domestic site's documentation; the Singapore
    // region has a different model lineup and its key doesn't work across sites, see the (Intl) preset
    name: "Tencent Token Plan Enterprise Pro",
    websiteUrl: "https://cloud.tencent.com/product/tokenhub",
    apiKeyUrl: "https://console.cloud.tencent.com/tokenhub/tokenplan-e",
    settingsConfig: {
      env: {
        ANTHROPIC_BASE_URL: "https://tokenhub.tencentmaas.com/plan/anthropic",
        ANTHROPIC_AUTH_TOKEN: "",
        ANTHROPIC_MODEL: "auto",
        ANTHROPIC_DEFAULT_HAIKU_MODEL: "auto",
        ANTHROPIC_DEFAULT_SONNET_MODEL: "auto",
        ANTHROPIC_DEFAULT_OPUS_MODEL: "auto",
      },
    },
    category: "cn_official",
    // The Guangzhou region is the default endpoint; the domestic-site enterprise package can also
    // select the Singapore region (dual-region table for 1823/130659 and 131173:
    // tokenhub-intl.tencentmaas.com, requires enabling the Singapore region;
    // cross-region calls are not supported, so it is only listed as a candidate endpoint)
    endpointCandidates: [
      "https://tokenhub.tencentmaas.com/plan/anthropic",
      "https://tokenhub-intl.tencentmaas.com/plan/anthropic",
    ],
    icon: "tencent",
    iconColor: "#0052D9",
  },
  {
    // International site Enterprise Pro package (intl 1300/81489, 2026-08-26 edition): Singapore
    // region, whose model lineup is a subset of the Guangzhou region's (no GLM-5/5.1/5-Turbo, Kimi-K2.6,
    // MiniMax-M2.7)
    name: "Tencent Token Plan Enterprise Pro (Intl)",
    websiteUrl: "https://www.tencentcloud.com/products/tokenhub",
    apiKeyUrl: "https://console.tencentcloud.com/tokenhub/tokenplan-e",
    settingsConfig: {
      env: {
        ANTHROPIC_BASE_URL:
          "https://tokenhub-intl.tencentcloudmaas.com/plan/anthropic",
        ANTHROPIC_AUTH_TOKEN: "",
        ANTHROPIC_MODEL: "auto",
        ANTHROPIC_DEFAULT_HAIKU_MODEL: "auto",
        ANTHROPIC_DEFAULT_SONNET_MODEL: "auto",
        ANTHROPIC_DEFAULT_OPUS_MODEL: "auto",
      },
    },
    category: "cn_official",
    // The Singapore region is the default endpoint; the international-site enterprise package can
    // also select the Guangzhou region (dual-region table for 1300/81489 and 81490:
    // tokenhub.tencentcloudmaas.com, requires enabling the Guangzhou region;
    // cross-region calls are not supported, so it is only listed as a candidate endpoint)
    endpointCandidates: [
      "https://tokenhub-intl.tencentcloudmaas.com/plan/anthropic",
      "https://tokenhub.tencentcloudmaas.com/plan/anthropic",
    ],
    icon: "tencent",
    iconColor: "#0052D9",
  },
  {
    // Token Plan Enterprise Lite package (1823/131173, 2026-08-28 edition): Auto model only
    name: "Tencent Token Plan Enterprise Lite",
    websiteUrl: "https://cloud.tencent.com/product/tokenhub",
    apiKeyUrl: "https://console.cloud.tencent.com/tokenhub/tokenplan-e",
    settingsConfig: {
      env: {
        ANTHROPIC_BASE_URL: "https://tokenhub.tencentmaas.com/plan/anthropic",
        ANTHROPIC_AUTH_TOKEN: "",
        ANTHROPIC_MODEL: "auto",
        ANTHROPIC_DEFAULT_HAIKU_MODEL: "auto",
        ANTHROPIC_DEFAULT_SONNET_MODEL: "auto",
        ANTHROPIC_DEFAULT_OPUS_MODEL: "auto",
      },
    },
    category: "cn_official",
    // The Guangzhou region is the default endpoint; the domestic-site enterprise package can also
    // select the Singapore region (dual-region table for 1823/130659 and 131173:
    // tokenhub-intl.tencentmaas.com, requires enabling the Singapore region;
    // cross-region calls are not supported, so it is only listed as a candidate endpoint)
    endpointCandidates: [
      "https://tokenhub.tencentmaas.com/plan/anthropic",
      "https://tokenhub-intl.tencentmaas.com/plan/anthropic",
    ],
    icon: "tencent",
    iconColor: "#0052D9",
  },
  {
    // International site Enterprise Lite package (intl 1300/81490): Singapore region (resource
    // scheduling scope is Global), Auto model only
    name: "Tencent Token Plan Enterprise Lite (Intl)",
    websiteUrl: "https://www.tencentcloud.com/products/tokenhub",
    apiKeyUrl: "https://console.tencentcloud.com/tokenhub/tokenplan-e",
    settingsConfig: {
      env: {
        ANTHROPIC_BASE_URL:
          "https://tokenhub-intl.tencentcloudmaas.com/plan/anthropic",
        ANTHROPIC_AUTH_TOKEN: "",
        ANTHROPIC_MODEL: "auto",
        ANTHROPIC_DEFAULT_HAIKU_MODEL: "auto",
        ANTHROPIC_DEFAULT_SONNET_MODEL: "auto",
        ANTHROPIC_DEFAULT_OPUS_MODEL: "auto",
      },
    },
    category: "cn_official",
    // The Singapore region is the default endpoint; the international-site enterprise package can
    // also select the Guangzhou region (dual-region table for 1300/81489 and 81490:
    // tokenhub.tencentcloudmaas.com, requires enabling the Guangzhou region;
    // cross-region calls are not supported, so it is only listed as a candidate endpoint)
    endpointCandidates: [
      "https://tokenhub-intl.tencentcloudmaas.com/plan/anthropic",
      "https://tokenhub.tencentcloudmaas.com/plan/anthropic",
    ],
    icon: "tencent",
    iconColor: "#0052D9",
  },
  {
    name: "Zhipu GLM",
    websiteUrl: "https://open.bigmodel.cn",
    apiKeyUrl: "https://www.bigmodel.cn/claude-code",
    settingsConfig: {
      env: {
        ANTHROPIC_BASE_URL: "https://open.bigmodel.cn/api/anthropic",
        ANTHROPIC_AUTH_TOKEN: "",
        ANTHROPIC_MODEL: "glm-5.1",
        ANTHROPIC_DEFAULT_HAIKU_MODEL: "glm-5.1",
        ANTHROPIC_DEFAULT_SONNET_MODEL: "glm-5.1",
        ANTHROPIC_DEFAULT_OPUS_MODEL: "glm-5.1",
      },
    },
    category: "cn_official",
    icon: "zhipu",
    iconColor: "#0F62FE",
  },
  {
    name: "Zhipu GLM en",
    websiteUrl: "https://z.ai",
    apiKeyUrl: "https://z.ai/subscribe",
    settingsConfig: {
      env: {
        ANTHROPIC_BASE_URL: "https://api.z.ai/api/anthropic",
        ANTHROPIC_AUTH_TOKEN: "",
        ANTHROPIC_MODEL: "glm-5.1",
        ANTHROPIC_DEFAULT_HAIKU_MODEL: "glm-5.1",
        ANTHROPIC_DEFAULT_SONNET_MODEL: "glm-5.1",
        ANTHROPIC_DEFAULT_OPUS_MODEL: "glm-5.1",
      },
    },
    category: "cn_official",
    icon: "zhipu",
    iconColor: "#0F62FE",
  },
  {
    name: "Baidu Qianfan Coding Plan",
    websiteUrl: "https://cloud.baidu.com/product/qianfan_modelbuilder",
    apiKeyUrl:
      "https://console.bce.baidu.com/qianfan/ais/console/applicationConsole/application",
    settingsConfig: {
      env: {
        ANTHROPIC_BASE_URL: "https://qianfan.baidubce.com/anthropic/coding",
        ANTHROPIC_AUTH_TOKEN: "",
        ANTHROPIC_MODEL: "qianfan-code-latest",
        ANTHROPIC_DEFAULT_HAIKU_MODEL: "qianfan-code-latest",
        ANTHROPIC_DEFAULT_SONNET_MODEL: "qianfan-code-latest",
        ANTHROPIC_DEFAULT_OPUS_MODEL: "qianfan-code-latest",
      },
    },
    category: "cn_official",
    endpointCandidates: ["https://qianfan.baidubce.com/anthropic/coding"],
    icon: "baidu",
    iconColor: "#2932E1",
  },
  {
    // Token Plan Personal Edition: on sale replacing the Coding Plan since 2026-07-13 (existing
    // Coding Plan subscriptions remain usable until they expire, so the old preset stays). Model =
    // deepseek-v4-pro across every role per the official Claude Code integration page (2026-07-30
    // edition); the key is a subscription-page-specific key
    name: "Baidu Qianfan Token Plan",
    websiteUrl: "https://cloud.baidu.com/product/codingplan.html",
    apiKeyUrl: "https://console.bce.baidu.com/qianfan/resource/token-plan",
    settingsConfig: {
      env: {
        ANTHROPIC_BASE_URL:
          "https://qianfan.baidubce.com/anthropic/tokenplan/personal",
        ANTHROPIC_AUTH_TOKEN: "",
        ANTHROPIC_MODEL: "deepseek-v4-pro",
        ANTHROPIC_DEFAULT_HAIKU_MODEL: "deepseek-v4-pro",
        ANTHROPIC_DEFAULT_SONNET_MODEL: "deepseek-v4-pro",
        ANTHROPIC_DEFAULT_OPUS_MODEL: "deepseek-v4-pro",
      },
    },
    category: "cn_official",
    endpointCandidates: [
      "https://qianfan.baidubce.com/anthropic/tokenplan/personal",
    ],
    icon: "baidu",
    iconColor: "#2932E1",
  },
  {
    name: "千问AI平台",
    websiteUrl: "https://platform.qianwenai.com/",
    apiKeyUrl: "https://platform.qianwenai.com/home/api-keys",
    settingsConfig: {
      env: {
        ANTHROPIC_BASE_URL: "https://dashscope.aliyuncs.com/apps/anthropic",
        ANTHROPIC_AUTH_TOKEN: "",
        ANTHROPIC_MODEL: "qwen3.8-max",
        ANTHROPIC_DEFAULT_HAIKU_MODEL: "qwen3.8-flash",
        ANTHROPIC_DEFAULT_SONNET_MODEL: "qwen3.7-plus",
        ANTHROPIC_DEFAULT_OPUS_MODEL: "qwen3.8-max",
        // When the model id isn't claude-*, Claude Code treats it with the 200K default window; the
        // official value must be pinned explicitly: qwen3.8 series context_window = 983616
        CLAUDE_CODE_MAX_CONTEXT_TOKENS: "983616",
      },
    },
    category: "cn_official",
    icon: "qianwenai",
    iconColor: "#624AFF",
  },
  {
    name: "千问AI平台 Coding Plan",
    websiteUrl: "https://bailian.console.aliyun.com",
    settingsConfig: {
      env: {
        ANTHROPIC_BASE_URL:
          "https://coding.dashscope.aliyuncs.com/apps/anthropic",
        ANTHROPIC_AUTH_TOKEN: "",
      },
    },
    category: "cn_official",
    icon: "qianwenai",
    iconColor: "#624AFF",
  },
  {
    name: "千问AI平台 Token Plan",
    websiteUrl: "https://platform.qianwenai.com/pricing/token-plan",
    apiKeyUrl: "https://platform.qianwenai.com/home/api-keys",
    settingsConfig: {
      env: {
        ANTHROPIC_BASE_URL:
          "https://token-plan.cn-beijing.maas.aliyuncs.com/apps/anthropic",
        ANTHROPIC_AUTH_TOKEN: "",
        ANTHROPIC_MODEL: "qwen3.8-max",
        ANTHROPIC_DEFAULT_HAIKU_MODEL: "qwen3.8-flash",
        ANTHROPIC_DEFAULT_SONNET_MODEL: "qwen3.7-plus",
        ANTHROPIC_DEFAULT_OPUS_MODEL: "qwen3.8-max",
        // The official Token Plan config likewise pins the window: qwen3.8 series context_window = 983616
        CLAUDE_CODE_MAX_CONTEXT_TOKENS: "983616",
      },
    },
    category: "cn_official",
    icon: "qianwenai",
    iconColor: "#624AFF",
  },
  // ===== QwenCloud (DashScope international site) =====
  // A separate site from the domestic Bailian entry above: domain, console, and API keys are not interchangeable.
  // Each of the three lines has its own dedicated base_url and API key; the official documentation
  // states explicitly that a key/base_url mismatch causes a 401, so this is split into three presets
  // rather than sharing one preset with extra candidate addresses.
  {
    name: "QwenCloud",
    websiteUrl: "https://home.qwencloud.com/",
    apiKeyUrl: "https://home.qwencloud.com/api-keys",
    settingsConfig: {
      env: {
        ANTHROPIC_BASE_URL:
          "https://dashscope-intl.aliyuncs.com/apps/anthropic",
        ANTHROPIC_AUTH_TOKEN: "",
        ANTHROPIC_MODEL: "qwen3.8-max",
        ANTHROPIC_DEFAULT_HAIKU_MODEL: "qwen3.8-flash",
        ANTHROPIC_DEFAULT_SONNET_MODEL: "qwen3.7-plus",
        ANTHROPIC_DEFAULT_OPUS_MODEL: "qwen3.8-max",
      },
    },
    category: "cn_official",
    icon: "qwencloud",
    iconColor: "#6336E7",
  },
  {
    name: "QwenCloud For Coding",
    websiteUrl: "https://www.qwencloud.com",
    apiKeyUrl: "https://home.qwencloud.com/api-keys",
    settingsConfig: {
      env: {
        ANTHROPIC_BASE_URL:
          "https://coding-intl.dashscope.aliyuncs.com/apps/anthropic",
        ANTHROPIC_AUTH_TOKEN: "",
        ANTHROPIC_MODEL: "qwen3.7-plus",
        ANTHROPIC_DEFAULT_HAIKU_MODEL: "qwen3.7-plus",
        ANTHROPIC_DEFAULT_SONNET_MODEL: "qwen3.7-plus",
        ANTHROPIC_DEFAULT_OPUS_MODEL: "qwen3.7-plus",
      },
    },
    category: "cn_official",
    icon: "qwencloud",
    iconColor: "#6336E7",
  },
  {
    name: "QwenCloud Token Plan",
    websiteUrl: "https://www.qwencloud.com/pricing/token-plan",
    apiKeyUrl: "https://home.qwencloud.com/api-keys",
    settingsConfig: {
      env: {
        ANTHROPIC_BASE_URL:
          "https://token-plan.ap-southeast-1.maas.aliyuncs.com/apps/anthropic",
        ANTHROPIC_AUTH_TOKEN: "",
        ANTHROPIC_MODEL: "qwen3.8-max",
        ANTHROPIC_DEFAULT_HAIKU_MODEL: "qwen3.8-flash",
        ANTHROPIC_DEFAULT_SONNET_MODEL: "qwen3.7-plus",
        ANTHROPIC_DEFAULT_OPUS_MODEL: "qwen3.8-max",
        // The window pinned by the official Claude Code config: qwen3.8 series context_window = 983616
        CLAUDE_CODE_MAX_CONTEXT_TOKENS: "983616",
      },
    },
    category: "cn_official",
    icon: "qwencloud",
    iconColor: "#6336E7",
  },
  {
    name: "StepFun",
    websiteUrl: "https://platform.stepfun.com/step-plan",
    apiKeyUrl: "https://platform.stepfun.com/interface-key",
    settingsConfig: {
      env: {
        ANTHROPIC_BASE_URL: "https://api.stepfun.com/step_plan",
        ANTHROPIC_AUTH_TOKEN: "",
        ANTHROPIC_MODEL: "step-3.5-flash-2603",
        ANTHROPIC_DEFAULT_HAIKU_MODEL: "step-3.5-flash-2603",
        ANTHROPIC_DEFAULT_SONNET_MODEL: "step-3.5-flash-2603",
        ANTHROPIC_DEFAULT_OPUS_MODEL: "step-3.5-flash-2603",
      },
    },
    category: "cn_official",
    endpointCandidates: ["https://api.stepfun.com/step_plan"],
    icon: "stepfun",
    iconColor: "#16D6D2",
  },
  {
    name: "StepFun en",
    websiteUrl: "https://platform.stepfun.ai/step-plan",
    apiKeyUrl: "https://platform.stepfun.ai/interface-key",
    settingsConfig: {
      env: {
        ANTHROPIC_BASE_URL: "https://api.stepfun.ai/step_plan",
        ANTHROPIC_AUTH_TOKEN: "",
        ANTHROPIC_MODEL: "step-3.5-flash-2603",
        ANTHROPIC_DEFAULT_HAIKU_MODEL: "step-3.5-flash-2603",
        ANTHROPIC_DEFAULT_SONNET_MODEL: "step-3.5-flash-2603",
        ANTHROPIC_DEFAULT_OPUS_MODEL: "step-3.5-flash-2603",
      },
    },
    category: "cn_official",
    endpointCandidates: ["https://api.stepfun.ai/step_plan"],
    icon: "stepfun",
    iconColor: "#16D6D2",
  },
  {
    name: "ModelScope",
    websiteUrl: "https://modelscope.cn",
    settingsConfig: {
      env: {
        ANTHROPIC_BASE_URL: "https://api-inference.modelscope.cn",
        ANTHROPIC_AUTH_TOKEN: "",
        ANTHROPIC_MODEL: "ZhipuAI/GLM-5.2",
        ANTHROPIC_DEFAULT_HAIKU_MODEL: "ZhipuAI/GLM-5.2",
        ANTHROPIC_DEFAULT_SONNET_MODEL: "ZhipuAI/GLM-5.2",
        ANTHROPIC_DEFAULT_OPUS_MODEL: "ZhipuAI/GLM-5.2",
      },
    },
    category: "aggregator",
    icon: "modelscope",
    iconColor: "#624AFF",
  },
  {
    name: "KAT-Coder",
    websiteUrl: "https://console.streamlake.ai",
    apiKeyUrl: "https://console.streamlake.ai/console/api-key",
    settingsConfig: {
      env: {
        ANTHROPIC_BASE_URL:
          "https://vanchin.streamlake.ai/api/gateway/v1/endpoints/${ENDPOINT_ID}/claude-code-proxy",
        ANTHROPIC_AUTH_TOKEN: "",
        ANTHROPIC_MODEL: "KAT-Coder-Pro V1",
        ANTHROPIC_DEFAULT_HAIKU_MODEL: "KAT-Coder-Air V1",
        ANTHROPIC_DEFAULT_SONNET_MODEL: "KAT-Coder-Pro V1",
        ANTHROPIC_DEFAULT_OPUS_MODEL: "KAT-Coder-Pro V1",
      },
    },
    category: "cn_official",
    templateValues: {
      ENDPOINT_ID: {
        label: "Vanchin Endpoint ID",
        placeholder: "ep-xxx-xxx",
        defaultValue: "",
        editorValue: "",
      },
    },
    icon: "catcoder",
  },
  {
    name: "Longcat",
    websiteUrl: "https://longcat.chat/platform",
    apiKeyUrl: "https://longcat.chat/platform/api_keys",
    settingsConfig: {
      env: {
        ANTHROPIC_BASE_URL: "https://api.longcat.chat/anthropic",
        ANTHROPIC_AUTH_TOKEN: "",
        ANTHROPIC_MODEL: "LongCat-2.0",
        ANTHROPIC_SMALL_FAST_MODEL: "LongCat-2.0",
        ANTHROPIC_DEFAULT_HAIKU_MODEL: "LongCat-2.0",
        ANTHROPIC_DEFAULT_SONNET_MODEL: "LongCat-2.0",
        ANTHROPIC_DEFAULT_OPUS_MODEL: "LongCat-2.0",
        CLAUDE_CODE_MAX_OUTPUT_TOKENS: "131072",
        CLAUDE_CODE_DISABLE_NONESSENTIAL_TRAFFIC: 1,
      },
    },
    category: "cn_official",
    icon: "longcat",
    iconColor: "#29E154",
  },
  {
    name: "MiniMax",
    websiteUrl: "https://platform.minimaxi.com",
    apiKeyUrl: "https://platform.minimaxi.com/subscribe/coding-plan",
    settingsConfig: {
      env: {
        ANTHROPIC_BASE_URL: "https://api.minimaxi.com/anthropic",
        ANTHROPIC_AUTH_TOKEN: "",
        API_TIMEOUT_MS: "3000000",
        CLAUDE_CODE_DISABLE_NONESSENTIAL_TRAFFIC: 1,
        CLAUDE_CODE_AUTO_COMPACT_WINDOW: "1000000",
        ANTHROPIC_MODEL: "MiniMax-M3[1M]",
        ANTHROPIC_DEFAULT_SONNET_MODEL: "MiniMax-M3[1M]",
        ANTHROPIC_DEFAULT_OPUS_MODEL: "MiniMax-M3[1M]",
        ANTHROPIC_DEFAULT_HAIKU_MODEL: "MiniMax-M3[1M]",
      },
    },
    category: "cn_official",
    theme: {
      backgroundColor: "#f64551",
      textColor: "#FFFFFF",
    },
    icon: "minimax",
    iconColor: "#FF6B6B",
  },
  {
    name: "MiniMax en",
    websiteUrl: "https://platform.minimax.io",
    apiKeyUrl: "https://platform.minimax.io/subscribe/coding-plan",
    settingsConfig: {
      env: {
        ANTHROPIC_BASE_URL: "https://api.minimax.io/anthropic",
        ANTHROPIC_AUTH_TOKEN: "",
        API_TIMEOUT_MS: "3000000",
        CLAUDE_CODE_DISABLE_NONESSENTIAL_TRAFFIC: 1,
        CLAUDE_CODE_AUTO_COMPACT_WINDOW: "1000000",
        ANTHROPIC_MODEL: "MiniMax-M3[1M]",
        ANTHROPIC_DEFAULT_SONNET_MODEL: "MiniMax-M3[1M]",
        ANTHROPIC_DEFAULT_OPUS_MODEL: "MiniMax-M3[1M]",
        ANTHROPIC_DEFAULT_HAIKU_MODEL: "MiniMax-M3[1M]",
      },
    },
    category: "cn_official",
    theme: {
      backgroundColor: "#f64551",
      textColor: "#FFFFFF",
    },
    icon: "minimax",
    iconColor: "#FF6B6B",
  },
  {
    name: "BaiLing",
    websiteUrl: "https://alipaytbox.yuque.com/sxs0ba/ling/get_started",
    settingsConfig: {
      env: {
        ANTHROPIC_BASE_URL: "https://api.tbox.cn/api/anthropic",
        ANTHROPIC_AUTH_TOKEN: "",
        ANTHROPIC_MODEL: "Ling-2.5-1T",
        ANTHROPIC_DEFAULT_HAIKU_MODEL: "Ling-2.5-1T",
        ANTHROPIC_DEFAULT_SONNET_MODEL: "Ling-2.5-1T",
        ANTHROPIC_DEFAULT_OPUS_MODEL: "Ling-2.5-1T",
      },
    },
    category: "cn_official",
  },
  {
    name: "AiHubMix",
    websiteUrl: "https://aihubmix.com",
    apiKeyUrl: "https://aihubmix.com",
    // Note: this provider uses ANTHROPIC_API_KEY (rather than ANTHROPIC_AUTH_TOKEN)
    apiKeyField: "ANTHROPIC_API_KEY",
    settingsConfig: {
      env: {
        ANTHROPIC_BASE_URL: "https://aihubmix.com",
        ANTHROPIC_API_KEY: "",
      },
    },
    // Candidate request addresses (for address management/speed testing); users can choose/override
    endpointCandidates: ["https://aihubmix.com", "https://api.aihubmix.com"],
    category: "aggregator",
    icon: "aihubmix",
    iconColor: "#006FFB",
  },
  {
    name: "CherryIN",
    websiteUrl: "https://open.cherryin.ai",
    apiKeyUrl: "https://open.cherryin.ai/console/token",
    settingsConfig: {
      env: {
        ANTHROPIC_BASE_URL: "https://open.cherryin.net",
        ANTHROPIC_AUTH_TOKEN: "",
        ANTHROPIC_MODEL: "anthropic/claude-sonnet-5",
        ANTHROPIC_DEFAULT_HAIKU_MODEL: "anthropic/claude-haiku-4.5",
        ANTHROPIC_DEFAULT_SONNET_MODEL: "anthropic/claude-sonnet-5",
        ANTHROPIC_DEFAULT_OPUS_MODEL: "anthropic/claude-opus-5",
      },
    },
    category: "aggregator",
    endpointCandidates: ["https://open.cherryin.net"],
    icon: "cherryin",
  },
  {
    name: "RelaxyCode",
    websiteUrl: "https://www.relaxycode.com",
    apiKeyUrl: "https://www.relaxycode.com/register",
    settingsConfig: {
      env: {
        ANTHROPIC_BASE_URL: "https://www.relaxycode.com",
        ANTHROPIC_AUTH_TOKEN: "",
      },
    },
    category: "third_party",
    icon: "relaxcode",
  },
  {
    name: "E-FlowCode",
    websiteUrl: "https://e-flowcode.cc",
    apiKeyUrl: "https://e-flowcode.cc",
    settingsConfig: {
      effortLevel: "high",
      env: {
        ANTHROPIC_AUTH_TOKEN: "",
        ANTHROPIC_BASE_URL: "https://e-flowcode.cc",
      },
      enabledPlugins: {
        "superpowers@superpowers-marketplace": true,
      },
      includeCoAuthoredBy: false,
      ENABLE_TOOL_SEARCH: true,
      skipWebFetchPreflight: true,
    },
    category: "third_party",
    endpointCandidates: ["https://e-flowcode.cc"],
    icon: "eflowcode",
    iconColor: "#000000",
  },
  {
    name: "OpenRouter",
    websiteUrl: "https://openrouter.ai",
    apiKeyUrl: "https://openrouter.ai/keys",
    settingsConfig: {
      env: {
        ANTHROPIC_BASE_URL: "https://openrouter.ai/api",
        ANTHROPIC_AUTH_TOKEN: "",
        ANTHROPIC_MODEL: "anthropic/claude-sonnet-5",
        ANTHROPIC_DEFAULT_HAIKU_MODEL: "anthropic/claude-haiku-4.5",
        ANTHROPIC_DEFAULT_SONNET_MODEL: "anthropic/claude-sonnet-5",
        ANTHROPIC_DEFAULT_OPUS_MODEL: "anthropic/claude-opus-5",
      },
    },
    category: "aggregator",
    icon: "openrouter",
    iconColor: "#6566F1",
  },
  {
    name: "TheRouter",
    websiteUrl: "https://therouter.ai",
    apiKeyUrl: "https://dashboard.therouter.ai",
    settingsConfig: {
      env: {
        ANTHROPIC_BASE_URL: "https://api.therouter.ai",
        ANTHROPIC_AUTH_TOKEN: "",
        ANTHROPIC_API_KEY: "",
        ANTHROPIC_MODEL: "anthropic/claude-sonnet-5",
        ANTHROPIC_DEFAULT_HAIKU_MODEL: "anthropic/claude-haiku-4.5",
        ANTHROPIC_DEFAULT_SONNET_MODEL: "anthropic/claude-sonnet-5",
        ANTHROPIC_DEFAULT_OPUS_MODEL: "anthropic/claude-opus-5",
      },
    },
    category: "aggregator",
    endpointCandidates: ["https://api.therouter.ai"],
  },
  {
    name: "Novita AI",
    websiteUrl: "https://novita.ai",
    apiKeyUrl: "https://novita.ai",
    settingsConfig: {
      env: {
        ANTHROPIC_BASE_URL: "https://api.novita.ai/anthropic",
        ANTHROPIC_AUTH_TOKEN: "",
        ANTHROPIC_MODEL: "zai-org/glm-5.1",
        ANTHROPIC_DEFAULT_HAIKU_MODEL: "zai-org/glm-5.1",
        ANTHROPIC_DEFAULT_SONNET_MODEL: "zai-org/glm-5.1",
        ANTHROPIC_DEFAULT_OPUS_MODEL: "zai-org/glm-5.1",
      },
    },
    category: "aggregator",
    endpointCandidates: ["https://api.novita.ai/anthropic"],
    // The Anthropic compatibility layer is at the /anthropic sub-path, while the OpenAI side is at
    // /openai/v1; the root path after stripping the suffix has no /models (verified 404), so the
    // generic candidate logic can't reach it, hence the override
    modelsUrl: "https://api.novita.ai/openai/v1/models",
    icon: "novita",
    iconColor: "#000000",
  },
  {
    name: "GitHub Copilot",
    websiteUrl: "https://github.com/features/copilot",
    settingsConfig: {
      env: {
        ANTHROPIC_BASE_URL: "https://api.githubcopilot.com",
        ANTHROPIC_MODEL: "claude-sonnet-5",
        ANTHROPIC_DEFAULT_HAIKU_MODEL: "claude-haiku-4.5",
        ANTHROPIC_DEFAULT_SONNET_MODEL: "claude-sonnet-5",
        ANTHROPIC_DEFAULT_OPUS_MODEL: "claude-sonnet-5",
      },
    },
    category: "third_party",
    apiFormat: "openai_chat",
    providerType: "github_copilot",
    requiresOAuth: true,
    icon: "github",
    iconColor: "#000000",
  },
  {
    name: "Codex",
    websiteUrl: "https://openai.com/chatgpt/pricing",
    settingsConfig: {
      env: {
        // base_url is forcibly rewritten by the proxy backend to chatgpt.com/backend-api/codex;
        // no user configuration is needed
        ANTHROPIC_BASE_URL: "https://chatgpt.com/backend-api/codex",
        ANTHROPIC_MODEL: "gpt-5.6-sol",
        ANTHROPIC_DEFAULT_HAIKU_MODEL: "gpt-5.6-luna",
        ANTHROPIC_DEFAULT_SONNET_MODEL: "gpt-5.6-sol",
        ANTHROPIC_DEFAULT_OPUS_MODEL: "gpt-5.6-sol",
        // Claude Code falls back to a 200K context window for unrecognized
        // non-Claude model ids. The ChatGPT Codex backend catalogs gpt-5.6-sol
        // at a 372K window with a ~353K effective budget (openai/codex#31860),
        // not the 1.05M API window. Pin both knobs: the compact window equals
        // min(model window, value), so matching the window is behavior-neutral
        // today but shields the compact trigger from remote-config experiments.
        // Tweak these directly in the JSON editor; no form fields on purpose.
        CLAUDE_CODE_MAX_CONTEXT_TOKENS: "372000",
        CLAUDE_CODE_AUTO_COMPACT_WINDOW: "372000",
      },
    },
    category: "third_party",
    apiFormat: "openai_responses",
    providerType: "codex_oauth",
    requiresOAuth: true,
    icon: "openai",
    iconColor: "#000000",
  },
  {
    name: "xAI (Grok)",
    websiteUrl: "https://x.ai/grok",
    settingsConfig: {
      env: {
        // The proxy enforces both this origin and the Responses wire format.
        ANTHROPIC_BASE_URL: "https://api.x.ai/v1",
        ANTHROPIC_MODEL: "grok-4.5",
        ANTHROPIC_DEFAULT_HAIKU_MODEL: "grok-4.5",
        ANTHROPIC_DEFAULT_SONNET_MODEL: "grok-4.5",
        ANTHROPIC_DEFAULT_OPUS_MODEL: "grok-4.5",
      },
    },
    category: "third_party",
    apiFormat: "openai_responses",
    providerType: "xai_oauth",
    requiresOAuth: true,
    icon: "xai",
    iconColor: "#000000",
  },
  {
    name: "Nvidia",
    websiteUrl: "https://build.nvidia.com",
    apiKeyUrl: "https://build.nvidia.com/settings/api-keys",
    settingsConfig: {
      env: {
        ANTHROPIC_BASE_URL: "https://integrate.api.nvidia.com",
        ANTHROPIC_AUTH_TOKEN: "",
        ANTHROPIC_MODEL: "moonshotai/kimi-k2.5",
        ANTHROPIC_DEFAULT_HAIKU_MODEL: "moonshotai/kimi-k2.5",
        ANTHROPIC_DEFAULT_SONNET_MODEL: "moonshotai/kimi-k2.5",
        ANTHROPIC_DEFAULT_OPUS_MODEL: "moonshotai/kimi-k2.5",
      },
    },
    category: "aggregator",
    apiFormat: "openai_chat",
    icon: "nvidia",
    iconColor: "#000000",
  },
  {
    name: "PIPELLM",
    websiteUrl: "https://code.pipellm.ai",
    apiKeyUrl: "https://code.pipellm.ai/login",
    settingsConfig: {
      env: {
        ANTHROPIC_BASE_URL: "https://cc-api.pipellm.ai",
        ANTHROPIC_AUTH_TOKEN: "",
        ANTHROPIC_MODEL: "claude-opus-5",
        ANTHROPIC_DEFAULT_HAIKU_MODEL: "claude-haiku-4-5-20251001",
        ANTHROPIC_DEFAULT_SONNET_MODEL: "claude-sonnet-5",
        ANTHROPIC_DEFAULT_OPUS_MODEL: "claude-opus-5",
      },
      includeCoAuthoredBy: false,
    },
    category: "aggregator",
    icon: "pipellm",
  },
  {
    name: "Xiaomi MiMo",
    websiteUrl: "https://platform.xiaomimimo.com",
    apiKeyUrl: "https://platform.xiaomimimo.com/#/console/api-keys",
    settingsConfig: {
      env: {
        ANTHROPIC_BASE_URL: "https://api.xiaomimimo.com/anthropic",
        ANTHROPIC_AUTH_TOKEN: "",
        ANTHROPIC_MODEL: "mimo-v2.5-pro",
        ANTHROPIC_DEFAULT_HAIKU_MODEL: "mimo-v2.5-pro",
        ANTHROPIC_DEFAULT_SONNET_MODEL: "mimo-v2.5-pro",
        ANTHROPIC_DEFAULT_OPUS_MODEL: "mimo-v2.5-pro",
      },
    },
    category: "cn_official",
    icon: "xiaomimimo",
    iconColor: "#000000",
  },
  {
    name: "Xiaomi MiMo Token Plan (China)",
    websiteUrl: "https://platform.xiaomimimo.com/#/token-plan",
    apiKeyUrl: "https://platform.xiaomimimo.com/#/console/plan-manage",
    settingsConfig: {
      env: {
        ANTHROPIC_BASE_URL: "https://token-plan-cn.xiaomimimo.com/anthropic",
        ANTHROPIC_AUTH_TOKEN: "",
        ANTHROPIC_MODEL: "mimo-v2.5-pro",
        ANTHROPIC_DEFAULT_HAIKU_MODEL: "mimo-v2.5-pro",
        ANTHROPIC_DEFAULT_SONNET_MODEL: "mimo-v2.5-pro",
        ANTHROPIC_DEFAULT_OPUS_MODEL: "mimo-v2.5-pro",
      },
    },
    category: "cn_official",
    icon: "xiaomimimo",
    iconColor: "#000000",
  },
  {
    name: "AWS Bedrock (AKSK)",
    websiteUrl: "https://aws.amazon.com/bedrock/",
    settingsConfig: {
      env: {
        ANTHROPIC_BASE_URL:
          "https://bedrock-runtime.${AWS_REGION}.amazonaws.com",
        AWS_ACCESS_KEY_ID: "${AWS_ACCESS_KEY_ID}",
        AWS_SECRET_ACCESS_KEY: "${AWS_SECRET_ACCESS_KEY}",
        AWS_REGION: "${AWS_REGION}",
        ANTHROPIC_MODEL: "global.anthropic.claude-opus-5",
        ANTHROPIC_DEFAULT_HAIKU_MODEL:
          "global.anthropic.claude-haiku-4-5-20251001-v1:0",
        ANTHROPIC_DEFAULT_SONNET_MODEL: "global.anthropic.claude-sonnet-5",
        ANTHROPIC_DEFAULT_OPUS_MODEL: "global.anthropic.claude-opus-5",
        CLAUDE_CODE_USE_BEDROCK: "1",
      },
    },
    category: "cloud_provider",
    templateValues: {
      AWS_REGION: {
        label: "AWS Region",
        placeholder: "us-west-2",
        editorValue: "us-west-2",
      },
      AWS_ACCESS_KEY_ID: {
        label: "Access Key ID",
        placeholder: "AKIA...",
        editorValue: "",
      },
      AWS_SECRET_ACCESS_KEY: {
        label: "Secret Access Key",
        placeholder: "your-secret-key",
        editorValue: "",
      },
    },
    icon: "aws",
    iconColor: "#FF9900",
  },
  {
    name: "AWS Bedrock (API Key)",
    websiteUrl: "https://aws.amazon.com/bedrock/",
    settingsConfig: {
      apiKey: "",
      env: {
        ANTHROPIC_BASE_URL:
          "https://bedrock-runtime.${AWS_REGION}.amazonaws.com",
        AWS_REGION: "${AWS_REGION}",
        ANTHROPIC_MODEL: "global.anthropic.claude-opus-5",
        ANTHROPIC_DEFAULT_HAIKU_MODEL:
          "global.anthropic.claude-haiku-4-5-20251001-v1:0",
        ANTHROPIC_DEFAULT_SONNET_MODEL: "global.anthropic.claude-sonnet-5",
        ANTHROPIC_DEFAULT_OPUS_MODEL: "global.anthropic.claude-opus-5",
        CLAUDE_CODE_USE_BEDROCK: "1",
      },
    },
    category: "cloud_provider",
    templateValues: {
      AWS_REGION: {
        label: "AWS Region",
        placeholder: "us-west-2",
        editorValue: "us-west-2",
      },
    },
    icon: "aws",
    iconColor: "#FF9900",
  },
  {
    name: "JieKou AI",
    websiteUrl: "https://jiekou.ai/#model-library",
    apiKeyUrl: "https://jiekou.ai/settings/key-management",
    settingsConfig: {
      env: {
        ANTHROPIC_BASE_URL: "https://api.jiekou.ai/anthropic",
        ANTHROPIC_AUTH_TOKEN: "",
        ANTHROPIC_MODEL: "claude-fable-5",
        ANTHROPIC_DEFAULT_HAIKU_MODEL: "claude-fable-5",
        ANTHROPIC_DEFAULT_SONNET_MODEL: "claude-fable-5",
        ANTHROPIC_DEFAULT_OPUS_MODEL: "claude-fable-5",
      },
    },
    category: "aggregator",
    endpointCandidates: ["https://api.jiekou.ai/anthropic"],
    // Same as Novita: Anthropic is at /anthropic, OpenAI is at /openai/v1, and the root path has
    // no /models (verified 404), so the generic candidate logic can't reach it, hence the override
    modelsUrl: "https://api.jiekou.ai/openai/v1/models",
    icon: "jiekou",
    iconColor: "#000000",
  },
  {
    name: "AICodeWith",
    websiteUrl: "https://aicodewith.ai",
    apiKeyUrl: "https://aicodewith.ai/login?tab=register",
    settingsConfig: {
      env: {
        ANTHROPIC_BASE_URL: "https://api.aicodewith.ai",
        ANTHROPIC_AUTH_TOKEN: "",
      },
    },
    category: "aggregator",
    endpointCandidates: ["https://api.aicodewith.ai"],
    icon: "aicodewith",
    iconColor: "#3A3B40",
  },
];
