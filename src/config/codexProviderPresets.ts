/**
 * Codex preset provider configuration templates
 */
import { ProviderCategory } from "../types";
import type {
  CodexApiFormat,
  CodexCatalogModel,
  CodexChatReasoning,
  PromptCacheRoutingMode,
} from "../types";
import type { PresetTheme } from "./claudeProviderPresets";

export interface CodexProviderPreset {
  name: string;
  nameKey?: string; // i18n key for localized display name
  websiteUrl: string;
  // Third-party providers can supply a separate link for obtaining an API Key
  apiKeyUrl?: string;
  auth: Record<string, string>; // Written to ~/.codex/auth.json
  config: string; // Written to ~/.codex/config.toml (TOML string)
  isOfficial?: boolean; // Marks whether this is an official preset
  category?: ProviderCategory; // Category
  isCustomTemplate?: boolean; // Marks whether this is a custom template
  // Candidate endpoint list (for address management / speed testing)
  endpointCandidates?: string[];
  // Visual theme configuration
  theme?: PresetTheme;
  // Icon configuration
  icon?: string; // Icon name
  iconColor?: string; // Icon color
  // Codex API format
  apiFormat?: CodexApiFormat;
  // Only used to distinguish preset origin; ChatGPT/Codex and xAI/Grok auth flows are independent of each other.
  providerType?: "codex_oauth" | "xai_oauth";
  // OAuth preset: hides the API Key input, requires the hosted account to already be logged in before saving
  requiresOAuth?: boolean;
  // Model catalog for Codex Chat local-routing mode
  modelCatalog?: CodexCatalogModel[];
  // Codex Responses -> Chat Completions reasoning capability defaults
  codexChatReasoning?: CodexChatReasoning;
  // Session-based prompt-cache routing override for Chat Completions upstreams
  promptCacheRouting?: PromptCacheRoutingMode;
}

/**
 * Generates auth.json for a third-party provider
 */
export function generateThirdPartyAuth(apiKey: string): Record<string, string> {
  return {
    OPENAI_API_KEY: apiKey || "",
  };
}

/**
 * Generates config.toml for a third-party provider
 */
export function generateThirdPartyConfig(
  providerName: string,
  baseUrl: string,
  modelName = "gpt-5.6-sol",
  options?: {
    // Hosted OAuth presets (requiresOAuth cards) must pass false: these cards have no static key,
    // and requires_openai_auth = true gets rejected by the backend's keyless safety gate on switch
    // (provider.codex.config.official_auth_fallback).
    requiresOpenAiAuth?: boolean;
  },
): string {
  const tomlString = (value: string) => JSON.stringify(value);
  const requiresOpenAiAuth = options?.requiresOpenAiAuth ?? true;

  return `model_provider = "custom"
model = ${tomlString(modelName)}
model_reasoning_effort = "high"
disable_response_storage = true

[model_providers.custom]
name = ${tomlString(providerName)}
base_url = ${tomlString(baseUrl)}
wire_api = "responses"
requires_openai_auth = ${requiresOpenAiAuth}`;
}

function modelCatalog(
  models: Array<
    | string
    | {
        model: string;
        displayName?: string;
        contextWindow?: number;
        // Native Responses (direct) overrides for the generated
        // model-catalogs.json. Omitted input modalities are inferred by the
        // backend: confirmed text-only models stay text-only; everything else
        // defaults to text+image.
        supportsParallelToolCalls?: boolean;
        inputModalities?: string[];
        // Vendor's OFFICIAL base_instructions; omit to inherit the neutral
        // template default. Required by Codex, so the backend always emits one.
        baseInstructions?: string;
        // Reasoning efforts the vendor's endpoint actually accepts (subset of
        // none/minimal/low/medium/high/xhigh/max/ultra). Omit to keep the
        // template's conservative none/high default. Pre-filled from official
        // vendor docs; users can still edit per provider in the form.
        reasoningLevels?: string[];
        defaultReasoningLevel?: string;
      }
  >,
): CodexCatalogModel[] {
  return models.map((entry) =>
    typeof entry === "string"
      ? { model: entry }
      : {
          model: entry.model,
          displayName: entry.displayName,
          contextWindow: entry.contextWindow,
          supportsParallelToolCalls: entry.supportsParallelToolCalls,
          inputModalities: entry.inputModalities,
          baseInstructions: entry.baseInstructions,
          reasoningLevels: entry.reasoningLevels,
          defaultReasoningLevel: entry.defaultReasoningLevel,
        },
  );
}

export const codexProviderPresets: CodexProviderPreset[] = [
  {
    name: "OpenAI Official",
    websiteUrl: "https://chatgpt.com/codex",
    isOfficial: true,
    category: "official",
    providerType: "codex_oauth",
    auth: {},
    config: ``,
    theme: {
      icon: "codex",
      backgroundColor: "#1F2937", // gray-800
      textColor: "#FFFFFF",
    },
    icon: "openai",
    iconColor: "#00A67E",
  },
  // ===== Pinned presets: file order = in-app display order =====
  {
    name: "Kimi",
    websiteUrl: "https://platform.kimi.com",
    apiKeyUrl: "https://platform.kimi.com/console/api-keys",
    auth: generateThirdPartyAuth(""),
    config: generateThirdPartyConfig(
      "kimi",
      "https://api.moonshot.cn/v1",
      "kimi-k3",
    ),
    endpointCandidates: ["https://api.moonshot.cn/v1"],
    // Native Responses direct connection (no local routing takeover needed): the
    // official Codex integration doc (platform.kimi.com/docs/guide/codex-kimi.md,
    // using CC Switch directly as the example) gives base_url =
    // https://api.moonshot.cn/v1 + wire_api = "responses", and explicitly states
    // the open platform "natively supports the Responses API used by Codex, no
    // protocol conversion or local proxy needed"; API reference
    // platform.kimi.com/docs/api/responses.md (POST /v1/responses,
    // reasoning.effort enum low/high/max, tool_choice only auto, supports
    // prompt_cache_key, usage includes cached_tokens). 2026-09-09 real-key
    // probe: Codex 0.153.4's full request shape (include
    // reasoning.encrypted_content + reasoning.summary + text.verbosity) and the
    // streaming event sequence both returned 200; kimi-k2.7-code also 200 -- the
    // docs only list kimi-k3, so this is undocumented capability; if the vendor
    // withdraws it, just delete the row from the catalog. The
    // thinking/reasoning_effort injection for the legacy openai_chat card comes
    // from the card's own meta.codexChatReasoning (no longer carried by the
    // preset); that code path's Kimi 400 still checks this injection first.
    apiFormat: "openai_responses",
    modelCatalog: modelCatalog([
      // First row = default model (catalog[0] must match config.toml's model):
      // kimi-k3 is the only model listed in both the official Codex doc and the
      // Responses OpenAPI spec (Jason's 2026-09-10 call, overturning the 07-17
      // ordering of "k3 after k2.7-code"). Tiers copied verbatim from the
      // official parameter doc (2026-08-15 audit, re-checked 2026-09-09): k3
      // cannot disable thinking, its three reasoning.effort tiers default to
      // max per the vendor; k2.7-code always thinks and the vendor states it
      // doesn't support reasoning_effort -> single tier high (avoids a fake
      // differentiated tier in the template, per the LongCat precedent). Neither
      // model can turn off thinking, so none is never listed. k3 doesn't
      // declare a default: the native template's default of high is in the
      // subset -> the backend keeps high, matching this preset's top-level
      // config.toml model_reasoning_effort = "high" (the value actually sent);
      // the catalog default only marks the Codex /model picker, and declaring
      // max would show a default that's never actually sent.
      // supportsParallelToolCalls is true on both rows: Kimi Code's official
      // catalog explicitly marks K3 as true, and both endpoints echoed
      // parallel_tool_calls back as true in the probe; leaving it unset would
      // make Codex <=0.148 users regress from the ProxyChat template's true to
      // the native template's false (the field no longer exists as of 0.153.4,
      // so setting it is harmless)
      {
        model: "kimi-k3",
        displayName: "Kimi K3",
        contextWindow: 1048576,
        supportsParallelToolCalls: true,
        reasoningLevels: ["low", "high", "max"],
      },
      {
        model: "kimi-k2.7-code",
        displayName: "Kimi K2.7 Code",
        contextWindow: 262144,
        supportsParallelToolCalls: true,
        reasoningLevels: ["high"],
      },
    ]),
    category: "cn_official",
    icon: "kimi",
    iconColor: "#6366F1",
  },
  {
    name: "Kimi For Coding",
    websiteUrl: "https://www.kimi.com/code/",
    apiKeyUrl: "https://www.kimi.com/code/",
    auth: generateThirdPartyAuth(""),
    config: generateThirdPartyConfig(
      "kimi_coding",
      "https://api.kimi.com/coding/v1",
      "kimi-for-coding",
    ),
    endpointCandidates: ["https://api.kimi.com/coding/v1"],
    // Native Responses direct connection (no local routing takeover needed): the
    // official Codex integration doc (kimi.com/code/docs/third-party-tools/codex.html,
    // using CC Switch as the example) gives base_url =
    // https://api.kimi.com/coding/v1 and states wire_api "must be set to
    // responses", explicitly stating "Kimi Code's server natively supports the
    // OpenAI Responses API (streaming/non-streaming, reasoning, and function
    // calling all work), no local routing or protocol conversion tool needed".
    // 2026-09-09 real-key probe: all four models returned 200 under Codex's
    // full request shape, with reasoning items carrying real
    // encrypted_content; a second request with the same prompt_cache_key hit
    // cached_tokens -- when directly connected, prompt_cache_key is sent by
    // Codex itself, so the conversion layer's promptCacheRouting re-injection
    // is no longer needed.
    apiFormat: "openai_responses",
    modelCatalog: modelCatalog([
      // Copied verbatim from the official models.json on the same page
      // (checked 2026-09-09): kimi-for-coding(-highspeed) = K2.7 Code, thinking
      // always ON with no tiers -> single tier high; k3/k3-256k have three
      // tiers and the vendor's default_reasoning_level = "high" (same as the
      // native template's fallback value; declared explicitly only so it's
      // visible in the form, per the MiniMax/MiMo precedent). none is not
      // listed -- on this gateway, disabling thinking silently routes to K2.6
      // (a different model with different billing). The gateway's effort
      // allowlist is ultra/max/xhigh/high/medium/low/minimum/light/none;
      // unknown values get a 400 (Codex's minimal isn't in the list; the tier
      // subset already blocks the picker from offering it, so users who change
      // the tier manually do so at their own risk). All four
      // supports_parallel_tool_calls rows are copied verbatim from the
      // vendor's true
      {
        model: "kimi-for-coding",
        displayName: "Kimi For Coding",
        contextWindow: 262144,
        supportsParallelToolCalls: true,
        reasoningLevels: ["high"],
      },
      {
        model: "kimi-for-coding-highspeed",
        displayName: "Kimi For Coding HighSpeed",
        contextWindow: 262144,
        supportsParallelToolCalls: true,
        reasoningLevels: ["high"],
      },
      {
        model: "k3",
        displayName: "Kimi K3",
        contextWindow: 1048576,
        supportsParallelToolCalls: true,
        reasoningLevels: ["low", "high", "max"],
        defaultReasoningLevel: "high",
      },
      {
        model: "k3-256k",
        displayName: "Kimi K3 256K",
        contextWindow: 262144,
        supportsParallelToolCalls: true,
        reasoningLevels: ["low", "high", "max"],
        defaultReasoningLevel: "high",
      },
    ]),
    category: "cn_official",
    icon: "kimi",
    iconColor: "#6366F1",
  },
  {
    name: "PackyCode",
    websiteUrl: "https://www.packyapi.ai",
    apiKeyUrl: "https://www.packyapi.ai/register",
    category: "third_party",
    auth: generateThirdPartyAuth(""),
    config: generateThirdPartyConfig(
      "packycode",
      "https://www.packyapi.ai/v1",
      "gpt-5.6-sol",
    ),
    endpointCandidates: [
      "https://www.packyapi.ai/v1",
      "https://cf.api.fan/v1",
      "https://slb-v1.api.fan/v1",
      "https://www.packyapi.com/v1",
    ],
    icon: "packycode",
  },
  {
    name: "ZetaAPI",
    websiteUrl: "https://zetaapi.ai",
    apiKeyUrl: "https://zetaapi.ai/go/u117",
    category: "aggregator",
    auth: generateThirdPartyAuth(""),
    config: generateThirdPartyConfig(
      "zetaapi",
      "https://api.zetaapi.ai/v1",
      "gpt-5.6-sol",
    ),
    endpointCandidates: ["https://api.zetaapi.ai/v1"],
    icon: "zetaapi",
  },
  {
    name: "APINebula",
    websiteUrl: "https://apinebula.ai",
    apiKeyUrl: "https://apinebula.ai/VjM74M",
    category: "third_party",
    auth: generateThirdPartyAuth(""),
    config: `model_provider = "custom"
model = "gpt-5.6-sol"
review_model = "gpt-5.6-sol"
model_reasoning_effort = "high"
disable_response_storage = true

[model_providers.custom]
name = "APINebula"
base_url = "https://apinebula.ai/v1"
wire_api = "responses"
requires_openai_auth = true`,
    endpointCandidates: ["https://apinebula.ai/v1"],
    apiFormat: "openai_responses",
    icon: "apinebula",
  },
  {
    name: "AICodeMirror",
    websiteUrl: "https://www.aicodemirror.ai",
    apiKeyUrl: "https://www.aicodemirror.ai/register",
    auth: generateThirdPartyAuth(""),
    config: generateThirdPartyConfig(
      "aicodemirror",
      "https://api.aicodemirror.ai/api/codex/backend-api/codex",
      "gpt-5.6-sol",
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
    category: "third_party",
    auth: generateThirdPartyAuth(""),
    config: generateThirdPartyConfig(
      "patewayai",
      "https://api.pateway.ai/v1",
      "gpt-5.6-sol",
    ),
    endpointCandidates: ["https://api.pateway.ai/v1"],
    icon: "pateway",
  },
  {
    name: "FennoAI",
    websiteUrl: "https://api.fenno.ai",
    apiKeyUrl:
      "https://api.fenno.ai/register?redirect=/purchase?tab=subscription%26group=16",
    category: "aggregator",
    auth: generateThirdPartyAuth(""),
    config: generateThirdPartyConfig(
      "fenno",
      "https://api.fenno.ai",
      "gpt-5.6-sol",
    ),
    endpointCandidates: ["https://api.fenno.ai"],
    icon: "fenno",
  },
  {
    name: "RunAPI",
    websiteUrl: "https://runapi.host",
    apiKeyUrl: "https://runapi.host/register",
    category: "aggregator",
    auth: generateThirdPartyAuth(""),
    config: generateThirdPartyConfig(
      "runapi",
      "https://runapi.host/v1",
      "gpt-5.6-sol",
    ),
    endpointCandidates: ["https://runapi.host/v1", "https://runapi.co/v1"],
    icon: "runapi",
  },
  {
    name: "Shengsuanyun",
    nameKey: "providerForm.presets.shengsuanyun",
    websiteUrl: "https://www.shengsuanyun.com/",
    apiKeyUrl: "https://www.shengsuanyun.com/",
    auth: generateThirdPartyAuth(""),
    config: generateThirdPartyConfig(
      "shengsuanyun",
      "https://router.shengsuanyun.com/api/v1",
      "openai/gpt-5.6-sol",
    ),
    category: "aggregator",
    icon: "shengsuanyun",
  },
  {
    name: "AIGoCode",
    websiteUrl: "https://aigocode.app",
    apiKeyUrl: "https://aigocode.app",
    category: "third_party",
    auth: generateThirdPartyAuth(""),
    config: generateThirdPartyConfig(
      "aigocode",
      "https://api.aigocode.app",
      "gpt-5.6-sol",
    ),
    endpointCandidates: ["https://api.aigocode.app"],
    icon: "aigocode",
    iconColor: "#5B7FFF",
  },
  {
    name: "Qiniu",
    nameKey: "providerForm.presets.qiniu",
    websiteUrl: "https://s.qiniu.com/nMvAvy",
    apiKeyUrl: "https://s.qiniu.com/nMvAvy",
    category: "aggregator",
    auth: generateThirdPartyAuth(""),
    config: generateThirdPartyConfig(
      "qiniu",
      "https://api.qnaigc.com/bypass/openai/v1",
      "gpt-5.6-sol",
    ),
    endpointCandidates: [
      "https://api.qnaigc.com/bypass/openai/v1",
      "https://api.modelink.ai/bypass/openai/v1",
    ],
    icon: "qiniu",
  },
  {
    name: "AICoding",
    websiteUrl: "https://aicoding.inc",
    apiKeyUrl: "https://aicoding.inc",
    auth: generateThirdPartyAuth(""),
    config: generateThirdPartyConfig(
      "aicoding",
      "https://api.aicoding.inc",
      "gpt-5.6-sol",
    ),
    endpointCandidates: ["https://api.aicoding.inc"],
    icon: "aicoding",
    iconColor: "#000000",
  },
  {
    name: "SubRouter",
    websiteUrl: "https://subrouter.ai",
    apiKeyUrl: "https://subrouter.ai/register",
    category: "aggregator",
    auth: generateThirdPartyAuth(""),
    config: generateThirdPartyConfig(
      "subrouter",
      "https://subrouter.ai/v1",
      "gpt-5.6-sol",
    ),
    endpointCandidates: ["https://subrouter.ai/v1"],
    icon: "subrouter",
  },
  {
    name: "APIKEY.FUN",
    websiteUrl: "https://apikey.fan",
    apiKeyUrl: "https://apikey.fan/register",
    category: "third_party",
    auth: generateThirdPartyAuth(""),
    config: `model_provider = "custom"
model = "gpt-5.6-sol"
review_model = "gpt-5.6-sol"
model_reasoning_effort = "high"
disable_response_storage = true

[model_providers.custom]
name = "APIKEY.FUN"
base_url = "https://api.apikey.fan/v1"
wire_api = "responses"
requires_openai_auth = true`,
    endpointCandidates: [
      "https://api.apikey.fan/v1",
      "https://api.apikey.fun/v1",
      "https://slb.apikey.fun/v1",
    ],
    apiFormat: "openai_responses",
    icon: "apikeyfun",
  },
  {
    name: "9527CODE",
    websiteUrl: "https://9527.codes",
    apiKeyUrl: "https://9527.codes/register",
    category: "aggregator",
    auth: generateThirdPartyAuth(""),
    config: generateThirdPartyConfig(
      "9527code",
      "https://9527.codes/v1",
      "gpt-5.6-sol",
    ),
    endpointCandidates: [
      "https://9527.codes/v1",
      "https://api.9527.codes/v1",
      "https://cdn.9527.codes/v1",
    ],
    icon: "9527code",
  },
  {
    name: "Code0",
    websiteUrl: "https://code0.ai",
    apiKeyUrl: "https://code0.ai/agent/register/B2XHxGjGmRvqgznY",
    category: "aggregator",
    auth: generateThirdPartyAuth(""),
    config: generateThirdPartyConfig(
      "code0",
      "https://code0.ai/v1",
      "gpt-5.6-sol",
    ),
    endpointCandidates: ["https://code0.ai/v1"],
    icon: "code0",
  },
  {
    name: "TeamoRouter",
    websiteUrl: "https://teamorouter.cn",
    apiKeyUrl: "https://teamorouter.cn/",
    category: "aggregator",
    auth: generateThirdPartyAuth(""),
    config: generateThirdPartyConfig(
      "teamorouter",
      "https://api.teamorouter.cn/v1",
      "gpt-5.6-sol",
    ),
    endpointCandidates: [
      "https://api.teamorouter.cn/v1",
      "https://api.teamorouter.com/v1",
    ],
    icon: "teamorouter",
  },
  {
    name: "PPIO",
    websiteUrl: "https://ppio.com",
    apiKeyUrl: "https://ppio.com",
    auth: generateThirdPartyAuth(""),
    config: generateThirdPartyConfig(
      "ppio",
      "https://api.ppio.com/openai/v1",
      "deepseek/deepseek-v4-flash-0731",
    ),
    endpointCandidates: ["https://api.ppio.com/openai/v1"],
    apiFormat: "openai_chat",
    modelCatalog: modelCatalog([
      {
        model: "deepseek/deepseek-v4-flash-0731",
        displayName: "Deepseek V4 Flash 0731",
        contextWindow: 1048576,
        inputModalities: ["text"],
      },
    ]),
    codexChatReasoning: {
      supportsThinking: true,
      supportsEffort: false,
      thinkingParam: "thinking",
      effortParam: "none",
      outputFormat: "reasoning_content",
    },
    category: "aggregator",
    icon: "ppio",
    iconColor: "#2874FF",
  },
  {
    name: "ClaudeCN",
    websiteUrl: "https://claudecn.top",
    apiKeyUrl: "https://claudecn.ai/register",
    category: "third_party",
    auth: generateThirdPartyAuth(""),
    config: generateThirdPartyConfig(
      "claudecn",
      "https://claudecn.top/v1",
      "gpt-5.6-sol",
    ),
    icon: "claudecn",
  },
  {
    name: "火山 Agent Plan",
    websiteUrl: "https://www.volcengine.com/activity/agentplan",
    apiKeyUrl: "https://www.volcengine.com/activity/agentplan",
    auth: generateThirdPartyAuth(""),
    config: generateThirdPartyConfig(
      "ark_agentplan",
      "https://ark.cn-beijing.volces.com/api/plan/v3",
      "ark-code-latest",
    ),
    // Billing red line (official warning): Agent Plan must go through
    // /api/plan/v3; the pay-as-you-go endpoint /api/v3 doesn't draw down the
    // plan quota and is billed separately, and Coding Plan's /api/coding/v3 is
    // a separate subscription -- neither must ever be mixed into the candidates
    endpointCandidates: ["https://ark.cn-beijing.volces.com/api/plan/v3"],
    // Official Codex doc (volcengine.com/docs/82379/2556056, updated 2026-07):
    // both Agent Plan /api/plan/v3 and Coding Plan /api/coding/v3 already
    // support the Responses API (wire_api=responses), no routing takeover
    // conversion needed
    apiFormat: "openai_responses",
    modelCatalog: modelCatalog([
      {
        model: "ark-code-latest",
        displayName: "Ark Code Latest",
        contextWindow: 256000,
        // All four official Codex integration docs (82379/2556054~2556057)
        // consistently restrict model_reasoning_effort to low/medium/high
        // only; none/xhigh/max are values exclusive to glm-5-2
        // (82379/1449737); since the alias can point to any backend model,
        // they can't be set here
        reasoningLevels: ["low", "medium", "high"],
      },
    ]),
    category: "cn_official",
    icon: "huoshan",
    iconColor: "#3370FF",
  },
  {
    name: "火山 Coding Plan",
    websiteUrl: "https://www.volcengine.com/activity/codingplan",
    apiKeyUrl: "https://www.volcengine.com/activity/codingplan",
    auth: generateThirdPartyAuth(""),
    config: generateThirdPartyConfig(
      "ark_codingplan",
      "https://ark.cn-beijing.volces.com/api/coding/v3",
      "ark-code-latest",
    ),
    // Billing red line (official warning): Coding Plan must go through
    // /api/coding/v3; the pay-as-you-go endpoint /api/v3 doesn't draw down the
    // plan quota and is billed separately, and Agent Plan's /api/plan/v3 is a
    // separate subscription -- neither must ever be mixed into the candidates
    endpointCandidates: ["https://ark.cn-beijing.volces.com/api/coding/v3"],
    // Official Codex doc (volcengine.com/docs/82379/2556056, updated 2026-07):
    // Coding Plan /api/coding/v3 already supports the Responses API
    // (wire_api=responses), no routing takeover conversion needed
    apiFormat: "openai_responses",
    modelCatalog: modelCatalog([
      {
        model: "ark-code-latest",
        displayName: "Ark Code Latest",
        contextWindow: 256000,
        // Same as Agent Plan: the official Codex doc locks this to the three
        // tiers low/medium/high
        reasoningLevels: ["low", "medium", "high"],
      },
    ]),
    category: "cn_official",
    icon: "huoshan",
    iconColor: "#3370FF",
  },
  {
    name: "BytePlus",
    websiteUrl: "https://www.byteplus.com/en/product/modelark",
    apiKeyUrl: "https://www.byteplus.com/en/product/modelark",
    auth: generateThirdPartyAuth(""),
    config: generateThirdPartyConfig(
      "byteplus",
      "https://ark.ap-southeast.bytepluses.com/api/coding/v3",
      "ark-code-latest",
    ),
    endpointCandidates: [
      "https://ark.ap-southeast.bytepluses.com/api/coding/v3",
    ],
    // Verified for the international site (2026-08-15 audit): BytePlus's
    // official Codex integration doc
    // (docs.byteplus.com/en/docs/ModelArk/2556056) shows the standard
    // config.toml's base_url is exactly this endpoint with
    // wire_api="responses", and the OpenCode doc likewise states Responses is
    // preferred -- aligned with the domestic Volcengine dual-Plan setup, so
    // this switches to a native direct connection
    apiFormat: "openai_responses",
    modelCatalog: modelCatalog([
      {
        model: "ark-code-latest",
        displayName: "Ark Code Latest",
        contextWindow: 256000,
        // The official Codex doc restricts model_reasoning_effort to
        // low/medium/high, cross-verified against the four docs for the
        // same-named model on the domestic site. ⚠️ The auto-routing alias has
        // inherent uncertainty: the vendor explicitly documents that when
        // routed to glm-5-2-260617, low/medium are treated as equivalent to
        // high
        reasoningLevels: ["low", "medium", "high"],
      },
    ]),
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
    auth: generateThirdPartyAuth(""),
    config: generateThirdPartyConfig(
      "doubaoseed",
      "https://ark.cn-beijing.volces.com/api/v3",
      "doubao-seed-2-1-pro-260628",
    ),
    endpointCandidates: ["https://ark.cn-beijing.volces.com/api/v3"],
    // Volcengine Ark's main data plane /api/v3 natively supports the Responses API (/api/v3/responses), no routing takeover conversion needed
    apiFormat: "openai_responses",
    // No official catalog: synthesize a MiMo-style entry (shell_command edits,
    // no freeform apply_patch), so Codex shows the model on a direct
    // connection and avoids the custom tool being rejected by the gateway
    modelCatalog: modelCatalog([
      {
        model: "doubao-seed-2-1-pro-260628",
        displayName: "Doubao Seed 2.1 Pro",
        contextWindow: 262144,
        // Of the 7-value enum in the Ark deep-thinking doc (82379/1449737),
        // these are the four generic tiers with no restriction for this
        // model; none/xhigh are glm-5-2 only, and max is marked as
        // Responses-support-pending on the deepseek list. minimal = Ark's
        // "disable thinking, answer directly" tier; the vendor states this
        // model's server defaults to high
        reasoningLevels: ["minimal", "low", "medium", "high"],
      },
    ]),
    category: "cn_official",
    icon: "doubao",
    iconColor: "#3370FF",
  },
  {
    name: "SiliconFlow",
    websiteUrl: "https://siliconflow.cn",
    apiKeyUrl: "https://cloud.siliconflow.cn",
    auth: generateThirdPartyAuth(""),
    config: generateThirdPartyConfig(
      "siliconflow",
      "https://api.siliconflow.cn/v1",
      "deepseek-ai/DeepSeek-V4-Flash",
    ),
    endpointCandidates: ["https://api.siliconflow.cn/v1"],
    apiFormat: "openai_chat",
    modelCatalog: modelCatalog([
      // The domestic M2.5 was retired on 2026-09-11; the V4 Flash currently
      // available on siliconflow.cn/models is the 0731 build. The platform's
      // Chat API only distinguishes high/max, so the original vendor's tiers
      // can't be copied verbatim:
      // https://docs.siliconflow.cn/docs/api/chat-completions-post
      {
        model: "deepseek-ai/DeepSeek-V4-Flash",
        displayName: "DeepSeek V4 Flash",
        contextWindow: 1048576,
        inputModalities: ["text"],
        reasoningLevels: ["high", "max"],
        defaultReasoningLevel: "high",
      },
    ]),
    // Explicitly overrides the platform's old supportsEffort:false inference, so the two tiers in the catalog actually get sent.
    codexChatReasoning: {
      supportsThinking: true,
      supportsEffort: true,
      thinkingParam: "enable_thinking",
      effortParam: "reasoning_effort",
      effortValueMode: "deepseek",
      outputFormat: "reasoning_content",
    },
    category: "aggregator",
    icon: "siliconflow",
    iconColor: "#6E29F6",
  },
  {
    name: "SiliconFlow en",
    websiteUrl: "https://siliconflow.com",
    apiKeyUrl: "https://cloud.siliconflow.cn",
    auth: generateThirdPartyAuth(""),
    config: generateThirdPartyConfig(
      "siliconflow_en",
      "https://api.siliconflow.com/v1",
      "MiniMaxAI/MiniMax-M3",
    ),
    endpointCandidates: ["https://api.siliconflow.com/v1"],
    apiFormat: "openai_chat",
    modelCatalog: modelCatalog([
      // 2026-08-15 audit: M2.7 was never listed on SiliconFlow; the .com site
      // swapped in M3 (the official page's 1M window = 1049K tokens; the .cn
      // site has no M3, don't cross-apply). SiliconFlow's platform switch is
      // the enable_thinking boolean (the backend falls back to platform
      // inference), and M3 can disable thinking -> two states
      {
        model: "MiniMaxAI/MiniMax-M3",
        displayName: "MiniMax M3",
        contextWindow: 1048576,
        reasoningLevels: ["none", "high"],
      },
    ]),
    category: "aggregator",
    icon: "siliconflow",
    iconColor: "#000000",
  },
  {
    name: "A6API",
    websiteUrl: "https://www.a6api.com",
    apiKeyUrl: "https://a6api.com/register",
    category: "aggregator",
    auth: generateThirdPartyAuth(""),
    config: generateThirdPartyConfig(
      "a6api",
      "https://api.a6api.com/v1",
      "gpt-5.6-sol",
    ),
    endpointCandidates: ["https://api.a6api.com/v1"],
    icon: "a6api",
  },
  {
    name: "Compshare",
    nameKey: "providerForm.presets.ucloud",
    websiteUrl: "https://www.compshare.cn",
    apiKeyUrl: "https://www.compshare.cn/coding-plan",
    auth: generateThirdPartyAuth(""),
    config: generateThirdPartyConfig(
      "compshare",
      "https://api.modelverse.cn/v1",
      "gpt-5.6-sol",
    ),
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
    auth: generateThirdPartyAuth(""),
    config: generateThirdPartyConfig(
      "compshare_coding",
      "https://cp.compshare.cn/v1",
      "gpt-5.6-sol",
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
    category: "aggregator",
    auth: generateThirdPartyAuth(""),
    config: generateThirdPartyConfig(
      "ccsub",
      "https://www.ccsub.net/v1",
      "gpt-5.6-sol",
    ),
    endpointCandidates: ["https://www.ccsub.net/v1"],
    icon: "ccsub",
  },
  {
    name: "SSSAiCode",
    websiteUrl: "https://sssaicodeapi.com",
    apiKeyUrl: "https://sssaicodeapi.com/register",
    auth: generateThirdPartyAuth(""),
    config: generateThirdPartyConfig(
      "sssaicode",
      "https://node-hk.sssaicodeapi.com/api/v1",
      "gpt-5.6-sol",
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
    category: "aggregator",
    auth: generateThirdPartyAuth(""),
    config: generateThirdPartyConfig(
      "soleapi",
      "https://soleapi.com/v1",
      "gpt-5.6-sol",
    ),
    endpointCandidates: ["https://soleapi.com/v1"],
    icon: "soleapi",
  },
  {
    name: "Micu",
    websiteUrl: "https://www.micuapi.ai",
    apiKeyUrl: "https://www.micuapi.ai/register",
    auth: generateThirdPartyAuth(""),
    config: generateThirdPartyConfig(
      "micu",
      "https://www.micuapi.ai/v1",
      "gpt-5.6-sol",
    ),
    endpointCandidates: ["https://www.micuapi.ai/v1"],
    category: "third_party",
    icon: "micu",
    iconColor: "#000000",
  },
  {
    name: "RightCode",
    websiteUrl: "https://www.rightapi.ai",
    apiKeyUrl: "https://www.rightapi.ai/register",
    auth: generateThirdPartyAuth(""),
    config: generateThirdPartyConfig(
      "rightcode",
      "https://www.rightapi.ai/codex/v1",
      "gpt-5.6-sol",
    ),
    category: "third_party",
    icon: "rc",
    iconColor: "#E96B2C",
  },
  {
    name: "ETok.ai",
    websiteUrl: "https://etok.ai",
    apiKeyUrl: "https://etok.ai",
    auth: generateThirdPartyAuth(""),
    config: generateThirdPartyConfig(
      "etok",
      "https://api.etok.ai/v1",
      "gpt-5.6-sol",
    ),
    endpointCandidates: ["https://api.etok.ai/v1"],
    category: "third_party",
    icon: "etok",
    iconColor: "#000000",
  },
  {
    name: "Cubence",
    websiteUrl: "https://cubence.com",
    apiKeyUrl: "https://cubence.com/signup",
    auth: generateThirdPartyAuth(""),
    config: generateThirdPartyConfig(
      "cubence",
      "https://api.cubence.com/v1",
      "gpt-5.6-sol",
    ),
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
    auth: generateThirdPartyAuth(""),
    config: generateThirdPartyConfig(
      "crazyrouter",
      "https://cn.crazyrouter.com/v1",
      "gpt-5.6-sol",
    ),
    endpointCandidates: ["https://cn.crazyrouter.com/v1"],
    icon: "crazyrouter",
    iconColor: "#000000",
  },
  {
    name: "DMXAPI",
    websiteUrl: "https://www.dmxapi.cn",
    category: "aggregator",
    auth: generateThirdPartyAuth(""),
    config: generateThirdPartyConfig(
      "dmxapi",
      "https://www.dmxapi.cn/v1",
      "gpt-5.6-sol",
    ),
    endpointCandidates: ["https://www.dmxapi.cn/v1"],
  },
  {
    name: "SudoCode.chat",
    websiteUrl: "https://sudocode.chat",
    apiKeyUrl: "https://sudocode.chat/sign-up",
    category: "third_party",
    auth: generateThirdPartyAuth(""),
    config: `model_provider = "custom"
model = "gpt-5.6-sol"
review_model = "gpt-5.6-sol"
model_reasoning_effort = "high"
disable_response_storage = true

[model_providers.custom]
name = "SudoCode"
base_url = "https://api.sudocode.chat/v1"
wire_api = "responses"
requires_openai_auth = true`,
    endpointCandidates: ["https://api.sudocode.chat/v1"],
    apiFormat: "openai_responses",
    icon: "sudocode",
  },
  {
    name: "SudoCode.us",
    websiteUrl: "https://sudocode.us",
    apiKeyUrl: "https://sudocode.us",
    category: "third_party",
    auth: generateThirdPartyAuth(""),
    config: `model_provider = "custom"
model = "gpt-5.6-sol"
review_model = "gpt-5.6-sol"
model_reasoning_effort = "high"
disable_response_storage = true
model_verbosity = "high"

[model_providers.custom]
name = "sudocode"
base_url = "https://sudocode.us/v1"
wire_api = "responses"
requires_openai_auth = true`,
    endpointCandidates: ["https://sudocode.us/v1", "https://sudocode.run/v1"],
    apiFormat: "openai_responses",
    icon: "sudocode-us",
  },
  {
    name: "XycAi",
    websiteUrl: "https://xycai.us",
    apiKeyUrl: "https://xycai.us/register",
    category: "aggregator",
    auth: generateThirdPartyAuth(""),
    config: generateThirdPartyConfig(
      "xycai",
      "https://apicdn.xycai.us/v1",
      "gpt-5.6-sol",
    ),
    endpointCandidates: [
      "https://apicdn.xycai.us/v1",
      "https://apicdn.xyc.ai/v1",
    ],
    icon: "xycai",
  },
  // ===== Remaining presets: displayed in-app sorted by display name; file order here doesn't affect display =====
  {
    name: "Amux",
    websiteUrl: "https://amux.ai",
    apiKeyUrl: "https://amux.ai",
    category: "aggregator",
    auth: generateThirdPartyAuth(""),
    config: generateThirdPartyConfig(
      "amux",
      "https://api.amux.ai/v1",
      "gpt-5.6-sol",
    ),
    endpointCandidates: ["https://api.amux.ai/v1"],
    icon: "amux",
  },
  {
    name: "AtlasCloud",
    websiteUrl: "https://www.atlascloud.ai/console/coding-plan",
    apiKeyUrl: "https://www.atlascloud.ai/console/coding-plan",
    category: "aggregator",
    auth: generateThirdPartyAuth(""),
    config: `model_provider = "custom"
model = "zai-org/glm-5.2"
disable_response_storage = true

[model_providers.custom]
name = "AtlasCloud"
base_url = "https://api.atlascloud.ai/v1"
wire_api = "responses"
requires_openai_auth = true`,
    endpointCandidates: ["https://api.atlascloud.ai/v1"],
    apiFormat: "openai_chat",
    modelCatalog: modelCatalog([
      // The latest GLM included in Coding Plan is 5.2 (2026-09-10); the
      // pay-as-you-go catalog's 5.3 is not covered by the plan. Context
      // window from https://api.atlascloud.ai/v1/models.
      {
        model: "zai-org/glm-5.2",
        displayName: "GLM 5.2",
        contextWindow: 1048576,
        inputModalities: ["text"],
        reasoningLevels: ["high"],
      },
    ]),
    // The platform hasn't confirmed this model's thinking-toggle/effort
    // contract; the single tier only signals thinking mode. Explicitly
    // overridden so the backend doesn't inject the vendor's native thinking
    // field based on the GLM model name.
    codexChatReasoning: {
      supportsThinking: false,
      supportsEffort: false,
      thinkingParam: "none",
      effortParam: "none",
      outputFormat: "reasoning_content",
    },
    icon: "atlascloud",
  },
  {
    name: "Azure OpenAI",
    websiteUrl:
      "https://learn.microsoft.com/en-us/azure/ai-foundry/openai/how-to/codex",
    category: "third_party",
    isOfficial: true,
    auth: generateThirdPartyAuth(""),
    config: `model_provider = "custom"
model = "gpt-5.6-sol"
model_reasoning_effort = "high"
disable_response_storage = true

[model_providers.custom]
name = "Azure OpenAI"
base_url = "https://YOUR_RESOURCE_NAME.openai.azure.com/openai"
env_key = "OPENAI_API_KEY"
query_params = { "api-version" = "2025-04-01-preview" }
wire_api = "responses"
requires_openai_auth = true`,
    endpointCandidates: ["https://YOUR_RESOURCE_NAME.openai.azure.com/openai"],
    theme: {
      icon: "codex",
      backgroundColor: "#0078D4",
      textColor: "#FFFFFF",
    },
    icon: "azure",
    iconColor: "#0078D4",
  },
  {
    name: "DeepSeek",
    websiteUrl: "https://platform.deepseek.com",
    apiKeyUrl: "https://platform.deepseek.com/api_keys",
    auth: generateThirdPartyAuth(""),
    config: generateThirdPartyConfig(
      "deepseek",
      "https://api.deepseek.com",
      "deepseek-v4-flash",
    ),
    endpointCandidates: ["https://api.deepseek.com"],
    // DeepSeek's official Codex doc (api-docs.deepseek.com ->
    // agent_integrations/codex): deepseek-v4-flash uses native Responses
    // (wire_api=responses against its own base_url), no routing takeover
    // conversion needed. The backend mirrors the official models.json
    // directly based on the deepseek.com host (freeform apply_patch + GPT-5
    // harness + low/high/max thinking tiers, requires codex >= 0.144.0); this
    // only keeps the row list and display names. Tiers copied verbatim from
    // the official catalog (low/high/max default high, 2026-08-15 re-check
    // confirmed flash/pro match byte-for-byte): per-row values override the
    // official mirror, so this must be kept in sync whenever DeepSeek's
    // official catalog changes (Jason's 2026-08-15 call: form visibility
    // takes priority over the risk of a stale snapshot; "unset" is more
    // misleading)
    apiFormat: "openai_responses",
    modelCatalog: modelCatalog([
      {
        model: "deepseek-v4-flash",
        displayName: "DeepSeek V4 Flash",
        contextWindow: 1048576,
        reasoningLevels: ["low", "high", "max"],
      },
      // pro had Responses/Codex integration enabled in 2026-08 (the official catalog entry differs from flash only in priority)
      {
        model: "deepseek-v4-pro",
        displayName: "DeepSeek V4 Pro",
        contextWindow: 1048576,
        reasoningLevels: ["low", "high", "max"],
      },
    ]),
    category: "cn_official",
    icon: "deepseek",
    iconColor: "#1E88E5",
  },
  {
    name: "Zhipu GLM",
    websiteUrl: "https://open.bigmodel.cn",
    apiKeyUrl: "https://www.bigmodel.cn/claude-code",
    auth: generateThirdPartyAuth(""),
    // Zhipu splits into three separate endpoints
    // (docs.bigmodel.cn/cn/coding-plan/tool/others): Anthropic
    // /api/anthropic, OpenAI Chat /api/coding/paas/v4, OpenAI Responses
    // /api/v1, and explicitly states "misconfiguring the endpoint will make
    // the GLM Coding Plan quota unusable". Codex direct-connect sends the
    // Responses wire, so base_url must be /api/v1; /responses on the Chat
    // endpoint is a strict legacy gateway (rejects type=custom tools -> the
    // 400 in #6944)
    config: generateThirdPartyConfig(
      "zhipu_glm",
      "https://open.bigmodel.cn/api/v1",
      "glm-5.3",
    ),
    endpointCandidates: ["https://open.bigmodel.cn/api/v1"],
    // Official Codex integration page
    // (docs.bigmodel.cn/cn/coding-plan/tool/codex, checked 2026-09-04):
    // wire_api=responses against its own /api/v1, a native direct connection
    // just like MiMo/MiniMax -> NativeResponses profile (shell_command edits,
    // no freeform apply_patch; although the official catalog declares
    // freeform, we take the conservative reading until verified on a real
    // device, to avoid introducing a 400)
    apiFormat: "openai_responses",
    // Tiers/context/modalities copied verbatim from the official
    // models.json: glm-5.3 is low/high/max, default max; glm-5-turbo's
    // official tier list is empty with default max -- cc-switch can't
    // express an empty tier list (falling back would produce the template's
    // none/high, and none has no conversion-layer fallback under a native
    // direct connection, so it would be sent as-is to the strict gateway), so
    // it's collapsed to the single official default tier max. Both models
    // have input_modalities=["text"] and parallel tool calls true
    modelCatalog: modelCatalog([
      {
        model: "glm-5.3",
        displayName: "GLM-5.3",
        contextWindow: 1048576,
        inputModalities: ["text"],
        supportsParallelToolCalls: true,
        reasoningLevels: ["low", "high", "max"],
        defaultReasoningLevel: "max",
      },
      {
        model: "glm-5-turbo",
        displayName: "GLM-5-Turbo",
        contextWindow: 204800,
        inputModalities: ["text"],
        supportsParallelToolCalls: true,
        reasoningLevels: ["max"],
      },
    ]),
    category: "cn_official",
    icon: "zhipu",
    iconColor: "#0F62FE",
  },
  {
    name: "Zhipu GLM en",
    websiteUrl: "https://z.ai",
    apiKeyUrl: "https://z.ai/subscribe",
    auth: generateThirdPartyAuth(""),
    // Same as above for the international site
    // (docs.z.ai/devpack/tool/others + devpack/tool/codex, checked
    // 2026-09-04): Responses endpoint /api/v1, the official models.json
    // lists only glm-5.3
    config: generateThirdPartyConfig(
      "zhipu_glm_en",
      "https://api.z.ai/api/v1",
      "glm-5.3",
    ),
    endpointCandidates: ["https://api.z.ai/api/v1"],
    apiFormat: "openai_responses",
    modelCatalog: modelCatalog([
      {
        model: "glm-5.3",
        displayName: "GLM-5.3",
        contextWindow: 1048576,
        inputModalities: ["text"],
        supportsParallelToolCalls: true,
        reasoningLevels: ["low", "high", "max"],
        defaultReasoningLevel: "max",
      },
    ]),
    category: "cn_official",
    icon: "zhipu",
    iconColor: "#0F62FE",
  },
  {
    name: "Baidu Qianfan Coding Plan",
    websiteUrl: "https://cloud.baidu.com/product/qianfan_modelbuilder",
    apiKeyUrl:
      "https://console.bce.baidu.com/qianfan/ais/console/applicationConsole/application",
    auth: generateThirdPartyAuth(""),
    config: generateThirdPartyConfig(
      "qianfan_coding",
      "https://qianfan.baidubce.com/v2/coding",
      "qianfan-code-latest",
    ),
    endpointCandidates: ["https://qianfan.baidubce.com/v2/coding"],
    apiFormat: "openai_chat",
    modelCatalog: modelCatalog([
      // Two states (2026-08-15 audit): Qianfan v2's official
      // thinking:{type:enabled/disabled} covers Coding Plan's six main
      // models, and the official OpenCode integration doc sends this field
      // verbatim for minimax-m2.5/glm-5/kimi-k2.5 on /v2/coding. ⚠️ Inherent
      // alias flaw: when the console resolves qianfan-code-latest to
      // ernie-4.5-turbo, none doesn't actually turn off thinking
      {
        model: "qianfan-code-latest",
        displayName: "Qianfan Code Latest",
        contextWindow: 131072,
        reasoningLevels: ["none", "high"],
      },
    ]),
    // Qianfan v2 Chat API's official top-level parameter (same shape as
    // Zhipu's); the platform "ignores unsupported parameters without
    // erroring" (documented in multiple places officially), so when the
    // alias resolves to a model not on the list, it just silently no-ops
    // instead of returning 400
    codexChatReasoning: {
      supportsThinking: true,
      supportsEffort: false,
      thinkingParam: "thinking",
      effortParam: "none",
      outputFormat: "reasoning_content",
    },
    category: "cn_official",
    icon: "baidu",
    iconColor: "#2932E1",
  },
  {
    // Token Plan (personal edition): replaced Coding Plan for new purchases
    // starting 2026-07-13 (Coding Plan stopped selling to new customers;
    // existing subscriptions remain usable until expiry, hence the old preset
    // above is kept). No alias mechanism -- specifies the real model id
    // directly; the official Codex integration guide omits wire_api = chat by
    // default, so this goes through local routing just like Coding Plan. The
    // API Key is a subscription-page-specific key (not a general-purpose app
    // key)
    name: "Baidu Qianfan Token Plan",
    websiteUrl: "https://cloud.baidu.com/product/codingplan.html",
    apiKeyUrl: "https://console.bce.baidu.com/qianfan/resource/token-plan",
    auth: generateThirdPartyAuth(""),
    config: generateThirdPartyConfig(
      "qianfan_tokenplan",
      "https://qianfan.baidubce.com/v2/tokenplan/personal",
      "deepseek-v4-pro",
    ),
    endpointCandidates: ["https://qianfan.baidubce.com/v2/tokenplan/personal"],
    apiFormat: "openai_chat",
    modelCatalog: modelCatalog([
      // Lineup and ordering = Token Plan (personal edition) doc (2026-08-14
      // version); ernie-5.1 is officially marked as retired on 8/20 and
      // excluded. Context window = Qianfan platform model list page
      // (2026-08-06 version; glm-5.1's 198000 is cross-confirmed by the
      // official OpenCode integration page)
      {
        // The DeepSeek V4 hosted on Qianfan Token Plan is a text-only
        // deployment (the Coding Plan doc explicitly states "image
        // understanding isn't supported yet"; attaching an image returns
        // 400), unlike the official endpoint where v4-flash routes to the
        // image-capable V4.1 Flash -- declare text-only explicitly, don't
        // rely on the global allowlist (#7283 follow-up)
        model: "deepseek-v4-pro",
        displayName: "DeepSeek V4 Pro",
        contextWindow: 1048576,
        // A model on both the official thinking and reasoning_effort lists:
        // none = thinking off, high/max = the vendor's only two real depth
        // tiers. No default declared: the vendor auto-sets max for complex
        // agent-style requests, which matches the fallback result -- pinning
        // high explicitly would suppress that platform behavior
        reasoningLevels: ["none", "high", "max"],
        inputModalities: ["text"],
      },
      {
        model: "deepseek-v4-flash",
        displayName: "DeepSeek V4 Flash",
        contextWindow: 1048576,
        reasoningLevels: ["none", "high", "max"],
        inputModalities: ["text"],
      },
      {
        // No separate entry on the platform's model list, and it's on
        // neither thinking list -- context window filled in to match
        // v4-flash; no evidence for tiers, so none are fabricated
        model: "deepseek-v4-flash-0731",
        displayName: "DeepSeek V4 Flash 0731",
        contextWindow: 1048576,
        inputModalities: ["text"],
      },
      {
        // Qianfan platform lists 1M (!= Zhipu's own coding endpoint's 200K
        // figure; context window is a property of the platform deployment);
        // not on the thinking list (2026-05-27 version), so no tier is set
        model: "glm-5.2",
        displayName: "GLM-5.2",
        contextWindow: 1048576,
      },
      {
        model: "glm-5.1",
        displayName: "GLM-5.1",
        contextWindow: 198000,
        // On the thinking list, and the official OpenCode integration page
        // sends thinking:{type:"enabled"} verbatim for it on the Token Plan
        // endpoint -> genuinely has two states
        reasoningLevels: ["none", "high"],
      },
      {
        // Not on the thinking list, so no tier is set
        model: "kimi-k2.6",
        displayName: "Kimi K2.6",
        contextWindow: 262144,
      },
    ]),
    // Difference from Coding Plan: supportsEffort is turned on here -- Coding
    // Plan keeps it false because the alias means it's unknown which model it
    // resolves to; Token Plan's catalog is all explicit models, and the
    // default model deepseek-v4-pro is on the official reasoning_effort list
    // (the list only covers v4-pro/v4-flash, with tiers limited to
    // high/max). effortValueMode:"deepseek" (max/xhigh/ultra -> max,
    // everything else -> high) matches Qianfan's official
    // backward-compatible mapping (low/medium -> high, xhigh -> max) exactly;
    // models not on the list receive reasoning_effort and, per the
    // platform's explicit documentation, it's "ignored without erroring", so
    // it's harmless
    codexChatReasoning: {
      supportsThinking: true,
      supportsEffort: true,
      thinkingParam: "thinking",
      effortParam: "reasoning_effort",
      effortValueMode: "deepseek",
      outputFormat: "reasoning_content",
    },
    category: "cn_official",
    icon: "baidu",
    iconColor: "#2932E1",
  },
  {
    name: "千问AI平台",
    websiteUrl: "https://platform.qianwenai.com/",
    apiKeyUrl: "https://platform.qianwenai.com/home/api-keys",
    auth: generateThirdPartyAuth(""),
    config: generateThirdPartyConfig(
      "qianwenai",
      "https://dashscope.aliyuncs.com/compatible-mode/v1",
      "qwen3.8-max",
    ),
    endpointCandidates: ["https://dashscope.aliyuncs.com/compatible-mode/v1"],
    // DashScope natively supports the OpenAI Responses API (/compatible-mode/v1/responses, same base_url), no routing takeover conversion needed
    apiFormat: "openai_responses",
    // Tiers and context window copied verbatim from the official Codex
    // model-catalog.local.json -- this metadata section isn't split by plan;
    // pay-as-you-go and Token Plan share the same one (the qwen3.8 line only
    // takes low/medium/xhigh, default xhigh; there's no high tier, don't pad
    // it out to the usual four tiers)
    modelCatalog: modelCatalog([
      {
        model: "qwen3.8-max",
        displayName: "Qwen3.8 Max",
        contextWindow: 983616,
        supportsParallelToolCalls: false,
        reasoningLevels: ["low", "medium", "xhigh"],
        defaultReasoningLevel: "xhigh",
      },
    ]),
    category: "cn_official",
    icon: "qianwenai",
    iconColor: "#624AFF",
  },
  {
    name: "千问AI平台 Token Plan",
    websiteUrl: "https://platform.qianwenai.com/pricing/token-plan",
    apiKeyUrl: "https://platform.qianwenai.com/home/api-keys",
    auth: generateThirdPartyAuth(""),
    config: generateThirdPartyConfig(
      "qianwenai_token_plan",
      "https://token-plan.cn-beijing.maas.aliyuncs.com/compatible-mode/v1",
      "qwen3.8-max",
    ),
    endpointCandidates: [
      "https://token-plan.cn-beijing.maas.aliyuncs.com/compatible-mode/v1",
    ],
    apiFormat: "openai_responses",
    // Tiers and context window copied verbatim from the official Codex
    // model-catalogs.json (the qwen3.8 line only takes low/medium/xhigh,
    // default xhigh; there's no high tier, don't pad it out to the usual four
    // tiers)
    modelCatalog: modelCatalog([
      {
        model: "qwen3.8-max",
        displayName: "Qwen3.8 Max",
        contextWindow: 983616,
        supportsParallelToolCalls: false,
        reasoningLevels: ["low", "medium", "xhigh"],
        defaultReasoningLevel: "xhigh",
      },
      {
        model: "qwen3.8-flash",
        displayName: "Qwen3.8 Flash",
        contextWindow: 983616,
        supportsParallelToolCalls: false,
        reasoningLevels: ["low", "medium", "xhigh"],
        defaultReasoningLevel: "xhigh",
      },
    ]),
    category: "cn_official",
    icon: "qianwenai",
    iconColor: "#624AFF",
  },
  // ===== QwenCloud (DashScope international site) =====
  // A separate site from the domestic entries above: domain, console, and
  // keys are not shared across sites.
  // Pay-as-you-go and Token Plan both use native Responses on /compatible-mode/v1.
  {
    name: "QwenCloud",
    websiteUrl: "https://home.qwencloud.com/",
    apiKeyUrl: "https://home.qwencloud.com/api-keys",
    auth: generateThirdPartyAuth(""),
    config: generateThirdPartyConfig(
      "qwencloud",
      "https://dashscope-intl.aliyuncs.com/compatible-mode/v1",
      "qwen3.8-max",
    ),
    endpointCandidates: [
      "https://dashscope-intl.aliyuncs.com/compatible-mode/v1",
    ],
    apiFormat: "openai_responses",
    // Tiers and context window copied verbatim from the official Codex
    // model-catalogs.json (the qwen3.8 line only takes low/medium/xhigh,
    // default xhigh; there's no high tier, don't pad it out to the usual four
    // tiers)
    modelCatalog: modelCatalog([
      {
        model: "qwen3.8-max",
        displayName: "Qwen3.8 Max",
        contextWindow: 983616,
        supportsParallelToolCalls: false,
        reasoningLevels: ["low", "medium", "xhigh"],
        defaultReasoningLevel: "xhigh",
      },
      {
        model: "qwen3.8-flash",
        displayName: "Qwen3.8 Flash",
        contextWindow: 983616,
        supportsParallelToolCalls: false,
        reasoningLevels: ["low", "medium", "xhigh"],
        defaultReasoningLevel: "xhigh",
      },
      {
        model: "qwen3.7-max",
        displayName: "Qwen3.7 Max",
        contextWindow: 1000000,
        inputModalities: ["text"],
      },
    ]),
    category: "cn_official",
    icon: "qwencloud",
    iconColor: "#6336E7",
  },
  {
    name: "QwenCloud For Coding",
    websiteUrl: "https://www.qwencloud.com",
    apiKeyUrl: "https://home.qwencloud.com/api-keys",
    auth: generateThirdPartyAuth(""),
    config: generateThirdPartyConfig(
      "qwencloud_coding",
      "https://coding-intl.dashscope.aliyuncs.com/v1",
      "qwen3.7-plus",
    ),
    endpointCandidates: ["https://coding-intl.dashscope.aliyuncs.com/v1"],
    apiFormat: "openai_chat",
    modelCatalog: modelCatalog([
      {
        model: "qwen3.7-plus",
        displayName: "Qwen3.7 Plus",
        contextWindow: 1000000,
      },
      {
        model: "qwen3.6-plus",
        displayName: "Qwen3.6 Plus",
        contextWindow: 1000000,
      },
      {
        model: "qwen3-coder-plus",
        displayName: "Qwen3 Coder Plus",
        contextWindow: 131072,
        inputModalities: ["text"],
      },
    ]),
    category: "cn_official",
    icon: "qwencloud",
    iconColor: "#6336E7",
  },
  {
    name: "QwenCloud Token Plan",
    websiteUrl: "https://www.qwencloud.com/pricing/token-plan",
    apiKeyUrl: "https://home.qwencloud.com/api-keys",
    auth: generateThirdPartyAuth(""),
    config: generateThirdPartyConfig(
      "qwencloud_token_plan",
      "https://token-plan.ap-southeast-1.maas.aliyuncs.com/compatible-mode/v1",
      "qwen3.8-max",
    ),
    endpointCandidates: [
      "https://token-plan.ap-southeast-1.maas.aliyuncs.com/compatible-mode/v1",
    ],
    apiFormat: "openai_responses",
    // Tiers and context window copied verbatim from the official Codex
    // model-catalogs.json (the qwen3.8 line only takes low/medium/xhigh,
    // default xhigh; there's no high tier, don't pad it out to the usual four
    // tiers)
    modelCatalog: modelCatalog([
      {
        model: "qwen3.8-max",
        displayName: "Qwen3.8 Max",
        contextWindow: 983616,
        supportsParallelToolCalls: false,
        reasoningLevels: ["low", "medium", "xhigh"],
        defaultReasoningLevel: "xhigh",
      },
      {
        model: "qwen3.8-flash",
        displayName: "Qwen3.8 Flash",
        contextWindow: 983616,
        supportsParallelToolCalls: false,
        reasoningLevels: ["low", "medium", "xhigh"],
        defaultReasoningLevel: "xhigh",
      },
      {
        model: "qwen3.7-max",
        displayName: "Qwen3.7 Max",
        contextWindow: 1000000,
        inputModalities: ["text"],
      },
    ]),
    category: "cn_official",
    icon: "qwencloud",
    iconColor: "#6336E7",
  },
  {
    name: "Tencent Hunyuan",
    websiteUrl: "https://cloud.tencent.com/product/tokenhub",
    apiKeyUrl: "https://console.cloud.tencent.com/tokenhub/apikey",
    auth: generateThirdPartyAuth(""),
    config: generateThirdPartyConfig(
      "hy3_tokenhub",
      "https://tokenhub.tencentmaas.com/v1",
      "hy3",
    ),
    // Official backup domain tencentmaas.cn (doc 1823/130078); the
    // international site's tokenhub-intl is a different region, and API Keys
    // aren't shared across sites, so it's not included as a candidate
    endpointCandidates: [
      "https://tokenhub.tencentmaas.com/v1",
      "https://tokenhub.tencentmaas.cn/v1",
    ],
    // Tencent TokenHub's official Codex doc
    // (cloud.tencent.com/document/product/1823/133532): hy3 uses native
    // Responses (wire_api=responses; the official hard requirement
    // disable_response_storage=true is already emitted by
    // generateThirdPartyConfig). ⚠️ Must use a TokenHub API Key (scope must
    // include Hy3 when creating it); Coding Plan / Token Plan subscription
    // keys only work against their own chat endpoints and won't authenticate
    // against this preset's /v1.
    // For requests that include tools, hy3's server automatically upgrades
    // reasoning_effort=low to high (Codex always sends tools), so the
    // default high is what actually happens.
    apiFormat: "openai_responses",
    // No official catalog: synthesize a MiMo-style entry (shell_command edits, no freeform apply_patch)
    modelCatalog: modelCatalog([
      {
        model: "hy3",
        displayName: "Hy3",
        contextWindow: 256000,
        // hy3 is not on the official image-understanding model list (1823/136956), text-only
        inputModalities: ["text"],
        // The official tier enum is only low/high (1823/131208 + the
        // open-weight chat template raises directly on other effort values);
        // when tools are present, low gets upgraded to high server-side
        reasoningLevels: ["low", "high"],
      },
      {
        model: "hy3-preview",
        displayName: "Hy3 Preview",
        contextWindow: 256000,
        inputModalities: ["text"],
        // Same as hy3: the official enum is low/high (1823/130930, interleaved thinking mode doc)
        reasoningLevels: ["low", "high"],
      },
    ]),
    category: "cn_official",
    icon: "hunyuan",
    iconColor: "#0055E9",
  },
  {
    // Tencent Cloud Token Plan (personal edition) (1823/130060, 2026-08-21
    // version): the general and Hy series share the same endpoint and API
    // Key, and the catalog merges both series; Auto's smart-routing call ID
    // is tc-code-latest. kimi-k2.5 is officially marked as retired on
    // 2026-08-31 and excluded (a real-key test still shows it working; it'll
    // be dropped once it actually expires). minimax-m2.5 isn't in the plan's
    // doc table but is listed in /plan/v3/models and works in a real-key
    // test, so it's included as-is.
    // Note this is a separate product line from the TokenHub pay-as-you-go
    // API marketplace (the 1823 line, the Hunyuan preset's /v1 endpoint):
    // subscription keys only work on the /plan endpoint, and TokenHub keys
    // don't authenticate against /plan
    name: "Tencent Token Plan",
    websiteUrl: "https://cloud.tencent.com/product/tokenhub",
    apiKeyUrl: "https://console.cloud.tencent.com/tokenhub/tokenplan",
    auth: generateThirdPartyAuth(""),
    config: generateThirdPartyConfig(
      "tencent_token_plan",
      "https://api.lkeap.cloud.tencent.com/plan/v3",
      "tc-code-latest",
    ),
    endpointCandidates: ["https://api.lkeap.cloud.tencent.com/plan/v3"],
    // Token Plan only offers Chat Completions; Codex needs local routing conversion
    apiFormat: "openai_chat",
    modelCatalog: modelCatalog([
      // Lineup = personal-edition doc (2026-08-21 version) + /models
      // real-key test. Context window = platform model list page (intl
      // 1300/78934, 2026-08-28 version). All thinking tiers verified with a
      // real key (2026-08-31): the thinking toggle genuinely works for
      // tc-code-latest/deepseek/GLM/hy3; minimax-m2.5/m2.7 can't be turned
      // off (the parameter is silently ignored, not an error), so only high
      // is listed to avoid a fake "select none but it still thinks" tier
      {
        model: "tc-code-latest",
        displayName: "Auto",
        // Auto's context window is only given on the official OpenClaw
        // integration page (1823/130062, 1300/81503): 196608; if omitted, the
        // backend falls back to the 128K default
        contextWindow: 196608,
        reasoningLevels: ["none", "high"],
      },
      {
        model: "deepseek-v4-flash-202605",
        displayName: "DeepSeek V4 Flash",
        contextWindow: 1048576,
        reasoningLevels: ["none", "high"],
      },
      {
        model: "deepseek-v4-pro-202606",
        displayName: "DeepSeek V4 Pro",
        contextWindow: 1048576,
        reasoningLevels: ["none", "high"],
      },
      {
        model: "minimax-m2.7",
        displayName: "MiniMax M2.7",
        contextWindow: 200000,
        reasoningLevels: ["high"],
      },
      {
        model: "glm-5",
        displayName: "GLM-5",
        contextWindow: 200000,
        reasoningLevels: ["none", "high"],
      },
      {
        model: "glm-5.1",
        displayName: "GLM-5.1",
        contextWindow: 200000,
        reasoningLevels: ["none", "high"],
      },
      {
        model: "glm-5.2",
        displayName: "GLM-5.2",
        contextWindow: 1048576,
        reasoningLevels: ["none", "high"],
      },
      // Hy Token Plan series (shares the endpoint and key with the general
      // series); calling hy3-preview auto-routes to hy3 (per official
      // announcement); text-only (hy3 is not on the 1823/136956 list)
      {
        model: "hy3",
        displayName: "Hy3",
        contextWindow: 256000,
        inputModalities: ["text"],
        reasoningLevels: ["none", "high"],
      },
      {
        model: "hy3-preview",
        displayName: "Hy3 Preview",
        contextWindow: 256000,
        inputModalities: ["text"],
        reasoningLevels: ["none", "high"],
      },
    ]),
    // Verified with a real key (2026-08-31): the thinking parameter genuinely
    // works on the /plan endpoint (both on/off verified; the thinking doc
    // 1300/80637 covers /plan); reasoning_effort is tolerated without error
    // on every model (including the default high). effortValueMode left
    // undeclared = passthrough; the tier value range is already restricted
    // to the real-key-verified safe set by each model's reasoningLevels
    codexChatReasoning: {
      supportsThinking: true,
      supportsEffort: true,
      thinkingParam: "thinking",
      effortParam: "reasoning_effort",
      outputFormat: "reasoning_content",
    },
    category: "cn_official",
    icon: "tencent",
    iconColor: "#0052D9",
  },
  {
    // International site (Singapore region) personal edition (intl
    // 1300/81315, 2026-08-20 version): Auto's call ID is auto (unlike the
    // domestic personal edition's tc-code-latest), and the lineup differs
    // from domestic (no GLM-5/5.1/Hy3, but adds GLM-5.2/MiniMax-M3). The
    // endpoint uses the tencentcloudmaas.com domain designated by the
    // international site's doc (DNS resolution in testing confirms a
    // Singapore node); the domestic site's doc gives
    // tokenhub-intl.tencentmaas.com for the Singapore region -- keys are
    // independent per site and not shared across sites, so neither is used
    // as a candidate for the other
    name: "Tencent Token Plan (Intl)",
    websiteUrl: "https://www.tencentcloud.com/products/tokenhub",
    apiKeyUrl: "https://console.tencentcloud.com/tokenhub/tokenplan",
    auth: generateThirdPartyAuth(""),
    config: generateThirdPartyConfig(
      "tencent_token_plan_intl",
      "https://tokenhub-intl.tencentcloudmaas.com/plan/v3",
      "auto",
    ),
    endpointCandidates: ["https://tokenhub-intl.tencentcloudmaas.com/plan/v3"],
    apiFormat: "openai_chat",
    modelCatalog: modelCatalog([
      // Thinking tiers verified with a real key (2026-08-31): INTL auto's
      // thinking toggle genuinely works (unlike domestic auto, which ignores
      // it); minimax-m3's thinking toggle also works
      {
        model: "auto",
        displayName: "Auto",
        // Auto's context window is only given on the official OpenClaw
        // integration page (1823/130062, 1300/81503): 196608; if omitted, the
        // backend falls back to the 128K default
        contextWindow: 196608,
        reasoningLevels: ["none", "high"],
      },
      {
        model: "glm-5.2",
        displayName: "GLM-5.2",
        contextWindow: 1048576,
        reasoningLevels: ["none", "high"],
      },
      {
        model: "kimi-k2.6",
        displayName: "Kimi K2.6",
        contextWindow: 262144,
        reasoningLevels: ["none", "high"],
      },
      {
        model: "deepseek-v4-pro-202606",
        displayName: "DeepSeek V4 Pro",
        contextWindow: 1048576,
        reasoningLevels: ["none", "high"],
      },
      {
        model: "deepseek-v4-flash-202605",
        displayName: "DeepSeek V4 Flash",
        contextWindow: 1048576,
        reasoningLevels: ["none", "high"],
      },
      {
        model: "minimax-m3",
        displayName: "MiniMax M3",
        contextWindow: 1048576,
        reasoningLevels: ["none", "high"],
      },
    ]),
    // thinking/reasoning_effort tested identical to the domestic personal edition (tolerated on every model, toggle works)
    codexChatReasoning: {
      supportsThinking: true,
      supportsEffort: true,
      thinkingParam: "thinking",
      effortParam: "reasoning_effort",
      outputFormat: "reasoning_content",
    },
    category: "cn_official",
    icon: "tencent",
    iconColor: "#0052D9",
  },
  {
    // Token Plan Enterprise Pro tier (1823/130659, 2026-08-25 version,
    // Guangzhou region): kimi-k2.5 is officially marked as retired on
    // 2026-08-31 and excluded; minimax-m2.5 has been removed from the model
    // list but still works in a real-key test (2026-08-31), so it's included
    // as-is. The Singapore region has a different lineup and keys aren't
    // shared across sites -- see the (Intl) preset
    name: "Tencent Token Plan Enterprise Pro",
    websiteUrl: "https://cloud.tencent.com/product/tokenhub",
    apiKeyUrl: "https://console.cloud.tencent.com/tokenhub/tokenplan-e",
    auth: generateThirdPartyAuth(""),
    config: generateThirdPartyConfig(
      "tencent_token_plan_enterprise_pro",
      "https://tokenhub.tencentmaas.com/plan/v3",
      "auto",
    ),
    // Guangzhou is the default region; the domestic site's enterprise tier
    // can also select the Singapore region (1823/130659, 131173 dual-region
    // table: tokenhub-intl.tencentmaas.com, requires enabling the Singapore
    // region; cross-region calls aren't supported, so it's only a candidate
    // endpoint)
    endpointCandidates: [
      "https://tokenhub.tencentmaas.com/plan/v3",
      "https://tokenhub-intl.tencentmaas.com/plan/v3",
    ],
    apiFormat: "openai_chat",
    modelCatalog: modelCatalog([
      // Lineup and ordering = enterprise Pro-tier doc's Guangzhou tab
      // (2026-08-25 version). All thinking tiers verified with a real key
      // (2026-08-31): glm-5.3 always thinks and its tier is a strict enum
      // low/high/max (medium/xhigh return a straight 400, and the error
      // message is the source of the enum); kimi-k2.7-code(-highspeed) only
      // accepts thinking:enabled; minimax-m2.5/m2.7's thinking-off is
      // silently ignored, and so is domestic auto's; every other model's
      // thinking toggle genuinely works
      {
        model: "auto",
        displayName: "Auto",
        // Auto's context window is only given on the official OpenClaw
        // integration page (1823/130062, 1300/81503): 196608; if omitted, the
        // backend falls back to the 128K default
        contextWindow: 196608,
        reasoningLevels: ["high"],
      },
      {
        model: "glm-5.3",
        displayName: "GLM-5.3",
        contextWindow: 1048576,
        reasoningLevels: ["low", "high", "max"],
        // Explicit default high: the template's default medium isn't in the
        // strict enum and would get dropped, falling back to
        // canonical.last()=max -- the slowest and most quota-hungry, not a
        // vendor-designated default (real-key testing found no evidence of a
        // no-param default), so high is chosen to match the plan's budget
        // orientation
        defaultReasoningLevel: "high",
      },
      {
        model: "glm-5.2",
        displayName: "GLM-5.2",
        contextWindow: 1048576,
        reasoningLevels: ["none", "high"],
      },
      {
        model: "glm-5",
        displayName: "GLM-5",
        contextWindow: 200000,
        reasoningLevels: ["none", "high"],
      },
      {
        model: "glm-5.1",
        displayName: "GLM-5.1",
        contextWindow: 200000,
        reasoningLevels: ["none", "high"],
      },
      {
        model: "glm-5-turbo",
        displayName: "GLM-5 Turbo",
        contextWindow: 200000,
        reasoningLevels: ["none", "high"],
      },
      {
        model: "kimi-k2.7-code",
        displayName: "Kimi K2.7 Code",
        contextWindow: 262144,
        reasoningLevels: ["high"],
      },
      {
        model: "kimi-k2.7-code-highspeed",
        displayName: "Kimi K2.7 Code HighSpeed",
        contextWindow: 262144,
        reasoningLevels: ["high"],
      },
      {
        model: "kimi-k2.6",
        displayName: "Kimi K2.6",
        contextWindow: 262144,
        reasoningLevels: ["none", "high"],
      },
      {
        model: "minimax-m2.7",
        displayName: "MiniMax M2.7",
        contextWindow: 200000,
        reasoningLevels: ["high"],
      },
      {
        model: "minimax-m3",
        displayName: "MiniMax M3",
        contextWindow: 1048576,
        reasoningLevels: ["none", "high"],
      },
      {
        // The DeepSeek V4 hosted on Tencent Token Plan is a text-only
        // deployment (the 2026-09-10 plan model list marks modality = text;
        // vision-exp is not included in this preset), unlike the official
        // endpoint where v4-flash routes to the image-capable V4.1 Flash --
        // declare text-only explicitly, don't rely on the global allowlist
        // (#7283 follow-up)
        model: "deepseek-v4-flash",
        displayName: "DeepSeek V4 Flash",
        contextWindow: 1048576,
        reasoningLevels: ["none", "high"],
        inputModalities: ["text"],
      },
      {
        model: "deepseek-v4-pro",
        displayName: "DeepSeek V4 Pro",
        contextWindow: 1048576,
        reasoningLevels: ["none", "high"],
        inputModalities: ["text"],
      },
      {
        model: "deepseek-v4-flash-0731",
        displayName: "DeepSeek V4 Flash 0731 GA",
        contextWindow: 1048576,
        reasoningLevels: ["none", "high"],
        inputModalities: ["text"],
      },
      {
        model: "deepseek-v4-pro-0813",
        displayName: "DeepSeek V4 Pro 0813 GA",
        contextWindow: 1048576,
        reasoningLevels: ["none", "high"],
        inputModalities: ["text"],
      },
      {
        model: "deepseek-v4-flash-202605",
        displayName: "DeepSeek V4 Flash Official",
        contextWindow: 1048576,
        reasoningLevels: ["none", "high"],
        inputModalities: ["text"],
      },
      {
        model: "deepseek-v4-pro-202606",
        displayName: "DeepSeek V4 Pro Official",
        contextWindow: 1048576,
        reasoningLevels: ["none", "high"],
        inputModalities: ["text"],
      },
    ]),
    // reasoning_effort default high is tolerated on every model in testing;
    // glm-5.3's medium/xhigh return 400, and the tier value range is already
    // restricted to the real-key-verified safe set by each model's
    // reasoningLevels
    codexChatReasoning: {
      supportsThinking: true,
      supportsEffort: true,
      thinkingParam: "thinking",
      effortParam: "reasoning_effort",
      outputFormat: "reasoning_content",
    },
    category: "cn_official",
    icon: "tencent",
    iconColor: "#0052D9",
  },
  {
    // International site enterprise Pro tier (intl 1300/81489, 2026-08-26
    // version, Singapore region): the lineup is a subset of the Guangzhou
    // region (no GLM-5/5.1/5-Turbo, Kimi-K2.6, MiniMax-M2.7)
    name: "Tencent Token Plan Enterprise Pro (Intl)",
    websiteUrl: "https://www.tencentcloud.com/products/tokenhub",
    apiKeyUrl: "https://console.tencentcloud.com/tokenhub/tokenplan-e",
    auth: generateThirdPartyAuth(""),
    config: generateThirdPartyConfig(
      "tencent_token_plan_enterprise_pro_intl",
      "https://tokenhub-intl.tencentcloudmaas.com/plan/v3",
      "auto",
    ),
    // Singapore is the default region; the international site's enterprise
    // tier can also select the Guangzhou region (1300/81489, 81490
    // dual-region table: tokenhub.tencentcloudmaas.com, requires enabling
    // the Guangzhou region; cross-region calls aren't supported, so it's
    // only a candidate endpoint)
    endpointCandidates: [
      "https://tokenhub-intl.tencentcloudmaas.com/plan/v3",
      "https://tokenhub.tencentcloudmaas.com/plan/v3",
    ],
    apiFormat: "openai_chat",
    modelCatalog: modelCatalog([
      // Lineup and ordering = international-site enterprise Pro-tier doc
      // (Singapore region). Thinking tiers verified with a real key
      // (2026-08-31): INTL auto's thinking-off genuinely works (unlike
      // domestic auto, which ignores it); glm-5.3 is a strict enum
      // low/high/max; kimi-k2.7-code(-highspeed) only accepts
      // thinking:enabled; every other model's toggle works
      {
        model: "auto",
        displayName: "Auto",
        // Auto's context window is only given on the official OpenClaw
        // integration page (1823/130062, 1300/81503): 196608; if omitted, the
        // backend falls back to the 128K default
        contextWindow: 196608,
        reasoningLevels: ["none", "high"],
      },
      {
        model: "glm-5.3",
        displayName: "GLM-5.3",
        contextWindow: 1048576,
        reasoningLevels: ["low", "high", "max"],
        // Explicit default high: the template's default medium isn't in the
        // strict enum and would get dropped, falling back to
        // canonical.last()=max -- the slowest and most quota-hungry, not a
        // vendor-designated default (real-key testing found no evidence of a
        // no-param default), so high is chosen to match the plan's budget
        // orientation
        defaultReasoningLevel: "high",
      },
      {
        model: "glm-5.2",
        displayName: "GLM-5.2",
        contextWindow: 1048576,
        reasoningLevels: ["none", "high"],
      },
      {
        model: "minimax-m3",
        displayName: "MiniMax M3",
        contextWindow: 1048576,
        reasoningLevels: ["none", "high"],
      },
      {
        model: "kimi-k2.7-code",
        displayName: "Kimi K2.7 Code",
        contextWindow: 262144,
        reasoningLevels: ["high"],
      },
      {
        model: "kimi-k2.7-code-highspeed",
        displayName: "Kimi K2.7 Code HighSpeed",
        contextWindow: 262144,
        reasoningLevels: ["high"],
      },
      {
        // Same as the domestic version: the DeepSeek V4 hosted on Tencent is
        // text-only; declared explicitly, don't rely on the global allowlist
        // (#7283 follow-up)
        model: "deepseek-v4-flash",
        displayName: "DeepSeek V4 Flash",
        contextWindow: 1048576,
        reasoningLevels: ["none", "high"],
        inputModalities: ["text"],
      },
      {
        model: "deepseek-v4-pro",
        displayName: "DeepSeek V4 Pro",
        contextWindow: 1048576,
        reasoningLevels: ["none", "high"],
        inputModalities: ["text"],
      },
      {
        model: "deepseek-v4-flash-0731",
        displayName: "DeepSeek V4 Flash 0731 GA",
        contextWindow: 1048576,
        reasoningLevels: ["none", "high"],
        inputModalities: ["text"],
      },
      {
        model: "deepseek-v4-pro-0813",
        displayName: "DeepSeek V4 Pro 0813 GA",
        contextWindow: 1048576,
        reasoningLevels: ["none", "high"],
        inputModalities: ["text"],
      },
      {
        model: "deepseek-v4-flash-202605",
        displayName: "DeepSeek V4 Flash Official",
        contextWindow: 1048576,
        reasoningLevels: ["none", "high"],
        inputModalities: ["text"],
      },
      {
        model: "deepseek-v4-pro-202606",
        displayName: "DeepSeek V4 Pro Official",
        contextWindow: 1048576,
        reasoningLevels: ["none", "high"],
        inputModalities: ["text"],
      },
    ]),
    // reasoning_effort default high is tolerated on every model in testing; same as the domestic enterprise Pro tier
    codexChatReasoning: {
      supportsThinking: true,
      supportsEffort: true,
      thinkingParam: "thinking",
      effortParam: "reasoning_effort",
      outputFormat: "reasoning_content",
    },
    category: "cn_official",
    icon: "tencent",
    iconColor: "#0052D9",
  },
  {
    // Token Plan Enterprise Lite tier (1823/131173, 2026-08-28 version):
    // Auto model only. Domestic auto's thinking-off is silently ignored
    // (real-key tested 2026-08-31), so only high is listed
    name: "Tencent Token Plan Enterprise Lite",
    websiteUrl: "https://cloud.tencent.com/product/tokenhub",
    apiKeyUrl: "https://console.cloud.tencent.com/tokenhub/tokenplan-e",
    auth: generateThirdPartyAuth(""),
    config: generateThirdPartyConfig(
      "tencent_token_plan_enterprise_lite",
      "https://tokenhub.tencentmaas.com/plan/v3",
      "auto",
    ),
    // Guangzhou is the default region; the domestic site's enterprise tier
    // can also select the Singapore region (1823/130659, 131173 dual-region
    // table: tokenhub-intl.tencentmaas.com, requires enabling the Singapore
    // region; cross-region calls aren't supported, so it's only a candidate
    // endpoint)
    endpointCandidates: [
      "https://tokenhub.tencentmaas.com/plan/v3",
      "https://tokenhub-intl.tencentmaas.com/plan/v3",
    ],
    apiFormat: "openai_chat",
    modelCatalog: modelCatalog([
      {
        model: "auto",
        displayName: "Auto",
        // Auto's context window is only given on the official OpenClaw
        // integration page (1823/130062, 1300/81503): 196608; if omitted, the
        // backend falls back to the 128K default
        contextWindow: 196608,
        reasoningLevels: ["high"],
      },
    ]),
    codexChatReasoning: {
      supportsThinking: true,
      supportsEffort: true,
      thinkingParam: "thinking",
      effortParam: "reasoning_effort",
      outputFormat: "reasoning_content",
    },
    category: "cn_official",
    icon: "tencent",
    iconColor: "#0052D9",
  },
  {
    // International site enterprise Lite tier (intl 1300/81490): Singapore
    // region (resource scheduling scope is Global), Auto model only. INTL
    // auto's thinking-off genuinely works (real-key tested)
    name: "Tencent Token Plan Enterprise Lite (Intl)",
    websiteUrl: "https://www.tencentcloud.com/products/tokenhub",
    apiKeyUrl: "https://console.tencentcloud.com/tokenhub/tokenplan-e",
    auth: generateThirdPartyAuth(""),
    config: generateThirdPartyConfig(
      "tencent_token_plan_enterprise_lite_intl",
      "https://tokenhub-intl.tencentcloudmaas.com/plan/v3",
      "auto",
    ),
    // Singapore is the default region; the international site's enterprise
    // tier can also select the Guangzhou region (1300/81489, 81490
    // dual-region table: tokenhub.tencentcloudmaas.com, requires enabling
    // the Guangzhou region; cross-region calls aren't supported, so it's
    // only a candidate endpoint)
    endpointCandidates: [
      "https://tokenhub-intl.tencentcloudmaas.com/plan/v3",
      "https://tokenhub.tencentcloudmaas.com/plan/v3",
    ],
    apiFormat: "openai_chat",
    modelCatalog: modelCatalog([
      {
        model: "auto",
        displayName: "Auto",
        // Auto's context window is only given on the official OpenClaw
        // integration page (1823/130062, 1300/81503): 196608; if omitted, the
        // backend falls back to the 128K default
        contextWindow: 196608,
        reasoningLevels: ["none", "high"],
      },
    ]),
    codexChatReasoning: {
      supportsThinking: true,
      supportsEffort: true,
      thinkingParam: "thinking",
      effortParam: "reasoning_effort",
      outputFormat: "reasoning_content",
    },
    category: "cn_official",
    icon: "tencent",
    iconColor: "#0052D9",
  },
  {
    name: "StepFun",
    websiteUrl: "https://platform.stepfun.com/step-plan",
    apiKeyUrl: "https://platform.stepfun.com/interface-key",
    auth: generateThirdPartyAuth(""),
    config: generateThirdPartyConfig(
      "stepfun",
      "https://api.stepfun.com/step_plan/v1",
      "step-3.7-flash",
    ),
    endpointCandidates: ["https://api.stepfun.com/step_plan/v1"],
    apiFormat: "openai_chat",
    modelCatalog: modelCatalog([
      // Tiers copied verbatim from the official model pages (both sites) and
      // the reasoning guide (2026-08-15 inventory): 3.7-flash has three
      // tiers defaulting to medium, 2603 has two; the unsuffixed 3.5-flash
      // doesn't officially expose effort, so it's left unset. None of the
      // family has a "no thinking" shape, so none is never listed. The
      // effort value sent downstream is inferred by the backend per model
      // (2603 converges to low_high, 3.7 passthrough); the preset doesn't
      // add codexChatReasoning -- declaring it at the provider level would
      // lose per-model gating
      {
        model: "step-3.7-flash",
        displayName: "Step 3.7 Flash",
        contextWindow: 262144,
        reasoningLevels: ["low", "medium", "high"],
      },
      {
        model: "step-3.5-flash-2603",
        displayName: "Step 3.5 Flash 2603",
        contextWindow: 262144,
        reasoningLevels: ["low", "high"],
      },
      {
        model: "step-3.5-flash",
        displayName: "Step 3.5 Flash",
        contextWindow: 262144,
      },
    ]),
    category: "cn_official",
    icon: "stepfun",
    iconColor: "#16D6D2",
  },
  {
    name: "StepFun en",
    websiteUrl: "https://platform.stepfun.ai/step-plan",
    apiKeyUrl: "https://platform.stepfun.ai/interface-key",
    auth: generateThirdPartyAuth(""),
    config: generateThirdPartyConfig(
      "stepfun_en",
      "https://api.stepfun.ai/step_plan/v1",
      "step-3.7-flash",
    ),
    endpointCandidates: ["https://api.stepfun.ai/step_plan/v1"],
    apiFormat: "openai_chat",
    modelCatalog: modelCatalog([
      // Tiers copied verbatim from the official model pages (both sites) and
      // the reasoning guide (2026-08-15 inventory): 3.7-flash has three
      // tiers defaulting to medium, 2603 has two; the unsuffixed 3.5-flash
      // doesn't officially expose effort, so it's left unset. None of the
      // family has a "no thinking" shape, so none is never listed. The
      // effort value sent downstream is inferred by the backend per model
      // (2603 converges to low_high, 3.7 passthrough); the preset doesn't
      // add codexChatReasoning -- declaring it at the provider level would
      // lose per-model gating
      {
        model: "step-3.7-flash",
        displayName: "Step 3.7 Flash",
        contextWindow: 262144,
        reasoningLevels: ["low", "medium", "high"],
      },
      {
        model: "step-3.5-flash-2603",
        displayName: "Step 3.5 Flash 2603",
        contextWindow: 262144,
        reasoningLevels: ["low", "high"],
      },
      {
        model: "step-3.5-flash",
        displayName: "Step 3.5 Flash",
        contextWindow: 262144,
      },
    ]),
    category: "cn_official",
    icon: "stepfun",
    iconColor: "#16D6D2",
  },
  {
    name: "ModelScope",
    websiteUrl: "https://modelscope.cn",
    apiKeyUrl: "https://modelscope.cn/my/myaccesstoken",
    auth: generateThirdPartyAuth(""),
    config: generateThirdPartyConfig(
      "modelscope",
      "https://api-inference.modelscope.cn/v1",
      "ZhipuAI/GLM-5.2",
    ),
    endpointCandidates: ["https://api-inference.modelscope.cn/v1"],
    apiFormat: "openai_chat",
    modelCatalog: modelCatalog([
      {
        // 2026-08-15 inventory: the bare GLM-5.1 id isn't in ModelScope's
        // free API-Inference list of 43 services (only the :DashScope
        // variant is, which needs a self-hosted Alibaba Cloud key), so use
        // GLM-5.2 instead (it's in the list, routed through the free
        // "ModelScope community" tier). Tier left unset: unconfirmed whether
        // ModelScope passes the thinking field through to upstream
        model: "ZhipuAI/GLM-5.2",
        displayName: "ZhipuAI / GLM-5.2",
        contextWindow: 200000,
      },
    ]),
    // Platform-dialect correction (2026-08-15 inventory): thinking:{type} is
    // Zhipu's own endpoint shape and appears nowhere in ModelScope's
    // platform docs; the platform's real toggle is the top-level
    // enable_thinking boolean (confirmed by both the official model page's
    // extra_body example and Bailian's first-party GLM docs). This whole
    // block must be kept -- removing it falls back to the backend's GLM
    // model-name inference and the wrong dialect comes back
    codexChatReasoning: {
      supportsThinking: true,
      supportsEffort: false,
      thinkingParam: "enable_thinking",
      effortParam: "none",
      outputFormat: "reasoning_content",
    },
    category: "aggregator",
    icon: "modelscope",
    iconColor: "#624AFF",
  },
  {
    name: "Longcat",
    websiteUrl: "https://longcat.chat/platform",
    apiKeyUrl: "https://longcat.chat/platform/api_keys",
    auth: generateThirdPartyAuth(""),
    config: generateThirdPartyConfig(
      "longcat",
      "https://api.longcat.chat/openai/v1",
      "LongCat-2.0",
    ),
    endpointCandidates: ["https://api.longcat.chat/openai/v1"],
    // Meituan LongCat's official Codex docs use wire_api=responses against
    // their own base_url -- native Responses, no routing takeover/conversion
    // needed
    apiFormat: "openai_responses",
    // No official catalog: synthesized MiMo-style (shell_command edits, no
    // freeform apply_patch). Note: LongCat's /responses tool-type contract
    // is the least documented of the bunch -- recommend a real-device smoke
    // test
    modelCatalog: modelCatalog([
      {
        model: "LongCat-2.0",
        displayName: "LongCat 2.0",
        contextWindow: 1048576,
        // LongCat has no adjustable tiers: the only effort evidence
        // site-wide is the "high" in the official Codex example; turning
        // thinking off goes through a different field,
        // thinking:{type:disabled} (the effort spelling isn't documented),
        // and the models API's supported_parameters doesn't include
        // reasoning either, so no fake "none" toggle is offered
        reasoningLevels: ["high"],
      },
    ]),
    category: "cn_official",
    icon: "longcat",
    iconColor: "#29E154",
  },
  {
    name: "MiniMax",
    websiteUrl: "https://platform.minimaxi.com",
    apiKeyUrl: "https://platform.minimaxi.com/subscribe/coding-plan",
    auth: generateThirdPartyAuth(""),
    config: generateThirdPartyConfig(
      "minimax",
      "https://api.minimaxi.com/v1",
      "MiniMax-M3",
    ),
    endpointCandidates: ["https://api.minimaxi.com/v1"],
    // MiniMax's official API reference already lists /v1/responses as a
    // formal endpoint (CN/intl dual-region, POST /v1/responses) -- native
    // Responses, no routing takeover/conversion needed
    apiFormat: "openai_responses",
    // Official Codex catalog (platform.minimaxi.com/docs/token-plan/codex-cli):
    // shell_command edits, parallel tools, text+image; doesn't declare
    // freeform apply_patch. Tiers copied verbatim from the official
    // catalog: none/high (M3's effort is a thinking toggle -- the endpoint
    // accepts minimal/low/medium but they behave identically to high, so no
    // fake differentiated tiers are given). Matches the template default,
    // so there's zero behavior change on the Codex side; declared
    // explicitly only so the form shows it ("unset" would be more
    // misleading -- Jason's call, 2026-08-15)
    modelCatalog: modelCatalog([
      {
        model: "MiniMax-M3",
        displayName: "MiniMax-M3",
        contextWindow: 1000000,
        reasoningLevels: ["none", "high"],
        supportsParallelToolCalls: true,
        inputModalities: ["text", "image"],
        baseInstructions:
          "You are Codex, a coding agent based on MiniMax-M3. You and the user share the same workspace and collaborate to achieve the user's goals.",
      },
    ]),
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
    auth: generateThirdPartyAuth(""),
    config: generateThirdPartyConfig(
      "minimax_en",
      "https://api.minimax.io/v1",
      "MiniMax-M3",
    ),
    endpointCandidates: ["https://api.minimax.io/v1"],
    // MiniMax's official API reference already lists /v1/responses as a
    // formal endpoint (CN/intl dual-region, POST /v1/responses) -- native
    // Responses, no routing takeover/conversion needed
    apiFormat: "openai_responses",
    // Official Codex catalog (platform.minimax.io/docs/token-plan/codex):
    // shell_command edits, parallel tools, text+image; doesn't declare
    // freeform apply_patch. Tiers copied verbatim from the official
    // catalog: none/high (M3's effort is a thinking toggle -- the endpoint
    // accepts minimal/low/medium but they behave identically to high, so no
    // fake differentiated tiers are given). Matches the template default,
    // so there's zero behavior change on the Codex side; declared
    // explicitly only so the form shows it ("unset" would be more
    // misleading -- Jason's call, 2026-08-15)
    modelCatalog: modelCatalog([
      {
        model: "MiniMax-M3",
        displayName: "MiniMax-M3",
        contextWindow: 1000000,
        reasoningLevels: ["none", "high"],
        supportsParallelToolCalls: true,
        inputModalities: ["text", "image"],
        baseInstructions:
          "You are Codex, a coding agent based on MiniMax-M3. You and the user share the same workspace and collaborate to achieve the user's goals.",
      },
    ]),
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
    apiKeyUrl: "https://ling.tbox.cn/open",
    auth: generateThirdPartyAuth(""),
    config: generateThirdPartyConfig(
      "bailing",
      "https://api.tbox.cn/api/llm/v1",
      "Ling-2.6-1T",
    ),
    endpointCandidates: ["https://api.tbox.cn/api/llm/v1"],
    apiFormat: "openai_chat",
    modelCatalog: modelCatalog([
      {
        model: "Ling-2.6-1T",
        displayName: "Ling-2.6-1T",
        contextWindow: 262144,
      },
    ]),
    category: "cn_official",
  },
  {
    name: "Xiaomi MiMo",
    websiteUrl: "https://platform.xiaomimimo.com",
    apiKeyUrl: "https://platform.xiaomimimo.com/#/console/api-keys",
    auth: generateThirdPartyAuth(""),
    config: generateThirdPartyConfig(
      "xiaomi_mimo",
      "https://api.xiaomimimo.com/v1",
      "mimo-v2.5-pro",
    ),
    endpointCandidates: ["https://api.xiaomimimo.com/v1"],
    // Xiaomi MiMo's official Codex docs already state native support for
    // the Responses API (wire_api=responses against their own base_url),
    // no routing takeover/conversion needed
    apiFormat: "openai_responses",
    // Official Codex catalog (mimo.mi.com/.../codex-configuration):
    // shell_command edits; doesn't declare freeform apply_patch.
    // Tiers copied verbatim from the official catalog: none/high (the
    // endpoint also accepts low/medium, but the official docs state all
    // three tiers "produce identical results, reasoning strength isn't
    // differentiated yet", so no fake differentiated tiers are given).
    // Matches the template default, so there's zero behavior change on the
    // Codex side; declared explicitly only so the form shows it ("unset"
    // would be more misleading -- Jason's call, 2026-08-15)
    modelCatalog: modelCatalog([
      {
        model: "mimo-v2.5-pro",
        displayName: "MiMo V2.5 Pro",
        contextWindow: 1048576,
        inputModalities: ["text"],
        reasoningLevels: ["none", "high"],
        baseInstructions:
          "You are MiMo, an AI assistant developed by Xiaomi. Today's date: {date} {week}. Your knowledge cutoff date is December 2024.",
      },
      {
        model: "mimo-v2.5",
        displayName: "MiMo V2.5",
        contextWindow: 1048576,
        inputModalities: ["text", "image"],
        reasoningLevels: ["none", "high"],
        baseInstructions:
          "You are MiMo, an AI assistant developed by Xiaomi. Today's date: {date} {week}. Your knowledge cutoff date is December 2024.",
      },
    ]),
    category: "cn_official",
    icon: "xiaomimimo",
    iconColor: "#000000",
  },
  {
    name: "Xiaomi MiMo Token Plan (China)",
    websiteUrl: "https://platform.xiaomimimo.com/#/token-plan",
    apiKeyUrl: "https://platform.xiaomimimo.com/#/console/plan-manage",
    auth: generateThirdPartyAuth(""),
    config: generateThirdPartyConfig(
      "xiaomi_mimo_token_plan",
      "https://token-plan-cn.xiaomimimo.com/v1",
      "mimo-v2.5-pro",
    ),
    endpointCandidates: ["https://token-plan-cn.xiaomimimo.com/v1"],
    // Xiaomi MiMo's official Codex docs already state native support for
    // the Responses API (wire_api=responses against their own base_url),
    // no routing takeover/conversion needed
    apiFormat: "openai_responses",
    // Official Codex catalog (mimo.mi.com/.../codex-configuration):
    // shell_command edits; doesn't declare freeform apply_patch.
    // Tiers copied verbatim from the official catalog: none/high (the
    // endpoint also accepts low/medium, but the official docs state all
    // three tiers "produce identical results, reasoning strength isn't
    // differentiated yet", so no fake differentiated tiers are given).
    // Matches the template default, so there's zero behavior change on the
    // Codex side; declared explicitly only so the form shows it ("unset"
    // would be more misleading -- Jason's call, 2026-08-15)
    modelCatalog: modelCatalog([
      {
        model: "mimo-v2.5-pro",
        displayName: "MiMo V2.5 Pro",
        contextWindow: 1048576,
        inputModalities: ["text"],
        reasoningLevels: ["none", "high"],
        baseInstructions:
          "You are MiMo, an AI assistant developed by Xiaomi. Today's date: {date} {week}. Your knowledge cutoff date is December 2024.",
      },
      {
        model: "mimo-v2.5",
        displayName: "MiMo V2.5",
        contextWindow: 1048576,
        inputModalities: ["text", "image"],
        reasoningLevels: ["none", "high"],
        baseInstructions:
          "You are MiMo, an AI assistant developed by Xiaomi. Today's date: {date} {week}. Your knowledge cutoff date is December 2024.",
      },
    ]),
    category: "cn_official",
    icon: "xiaomimimo",
    iconColor: "#000000",
  },
  {
    name: "Novita AI",
    websiteUrl: "https://novita.ai",
    apiKeyUrl: "https://novita.ai",
    auth: generateThirdPartyAuth(""),
    config: generateThirdPartyConfig(
      "novita",
      "https://api.novita.ai/openai/v1",
      "zai-org/glm-5.3",
    ),
    endpointCandidates: ["https://api.novita.ai/openai/v1"],
    apiFormat: "openai_chat",
    modelCatalog: modelCatalog([
      // Official available catalog (2026-09-10):
      // https://api.novita.ai/openai/v1/models GLM-5.3 is text-only, 1M
      // context; the platform hasn't confirmed enable_thinking/effort on a
      // per-model basis.
      {
        model: "zai-org/glm-5.3",
        displayName: "GLM-5.3",
        contextWindow: 1048576,
        inputModalities: ["text"],
        reasoningLevels: ["high"],
      },
    ]),
    // Don't carry over the older model's toggle inference; keep the
    // explicit override to stop the vendor's native parameter from being
    // injected based on model name.
    codexChatReasoning: {
      supportsThinking: false,
      supportsEffort: false,
      thinkingParam: "none",
      effortParam: "none",
      outputFormat: "reasoning_content",
    },
    category: "aggregator",
    icon: "novita",
    iconColor: "#000000",
  },
  {
    name: "xAI (Grok)",
    websiteUrl: "https://x.ai/api",
    apiKeyUrl: "https://console.x.ai",
    auth: generateThirdPartyAuth(""),
    config: generateThirdPartyConfig("xai", "https://api.x.ai/v1", "grok-4.5"),
    endpointCandidates: ["https://api.x.ai/v1"],
    // xAI officially treats /v1/responses as a first-class endpoint
    // (docs.x.ai api-reference): store:false / include=["reasoning.encrypted_content"]
    // / reasoning effort -- everything Codex hard-depends on is supported;
    // native Responses, no routing takeover/conversion needed
    apiFormat: "openai_responses",
    modelCatalog: modelCatalog([
      {
        model: "grok-4.5",
        displayName: "Grok 4.5",
        contextWindow: 500000,
        supportsParallelToolCalls: true,
        inputModalities: ["text", "image"],
        // Real-key tested (2026-08-30, probed tier-by-tier against native
        // /v1/responses): grok-4.5 accepts low/medium/high/xhigh, rejects
        // max (HTTP 400 "Invalid reasoning effort"); "Reasoning cannot be
        // disabled" so there's no none tier. Codex doesn't clamp
        // out-of-range tiers against the catalog (whatever the Desktop UI
        // picks, including max, gets sent as-is), so this list must match
        // what upstream actually accepts -- don't add/remove entries based
        // on the docs alone.
        // WARNING: the page at docs.x.ai/developers/grok-4-5 actually
        // renders grok-4.6 content -- don't cite it
        reasoningLevels: ["low", "medium", "high", "xhigh"],
      },
    ]),
    category: "third_party",
    icon: "xai",
    iconColor: "#000000",
  },
  {
    name: "xAI (Grok) OAuth",
    websiteUrl: "https://x.ai/grok",
    auth: generateThirdPartyAuth(""),
    // Managed OAuth: the real token is injected per-request by the local
    // proxy; CodexAdapter hard-routes to api.x.ai -- the base_url / empty
    // auth here is just a config snapshot and has no effect when forwarding.
    // requires_openai_auth must be false: keyless + true gets rejected by
    // the backend's safety gate when switching (the backend write layer
    // also forces existing cards back to false).
    config: generateThirdPartyConfig("xai", "https://api.x.ai/v1", "grok-4.5", {
      requiresOpenAiAuth: false,
    }),
    apiFormat: "openai_responses",
    providerType: "xai_oauth",
    requiresOAuth: true,
    modelCatalog: modelCatalog([
      {
        model: "grok-4.5",
        displayName: "Grok 4.5",
        contextWindow: 500000,
        supportsParallelToolCalls: true,
        inputModalities: ["text", "image"],
        // Real-key tested (2026-08-30, probed tier-by-tier against native
        // /v1/responses): grok-4.5 accepts low/medium/high/xhigh, rejects
        // max (HTTP 400 "Invalid reasoning effort"); "Reasoning cannot be
        // disabled" so there's no none tier. Codex doesn't clamp
        // out-of-range tiers against the catalog (whatever the Desktop UI
        // picks, including max, gets sent as-is), so this list must match
        // what upstream actually accepts -- don't add/remove entries based
        // on the docs alone.
        // WARNING: the page at docs.x.ai/developers/grok-4-5 actually
        // renders grok-4.6 content -- don't cite it
        reasoningLevels: ["low", "medium", "high", "xhigh"],
      },
    ]),
    category: "third_party",
    icon: "xai",
    iconColor: "#000000",
  },
  {
    name: "Nvidia",
    websiteUrl: "https://build.nvidia.com",
    apiKeyUrl: "https://build.nvidia.com/settings/api-keys",
    auth: generateThirdPartyAuth(""),
    config: generateThirdPartyConfig(
      "nvidia",
      "https://integrate.api.nvidia.com/v1",
      "moonshotai/kimi-k3",
    ),
    endpointCandidates: ["https://integrate.api.nvidia.com/v1"],
    apiFormat: "openai_chat",
    modelCatalog: modelCatalog([
      {
        model: "moonshotai/kimi-k3",
        displayName: "Kimi K3",
        contextWindow: 1048576,
        inputModalities: ["text", "image"],
        reasoningLevels: ["low", "high", "max"],
        defaultReasoningLevel: "high",
      },
    ]),
    // NIM K3 always thinks and only accepts reasoning_effort:
    // low/high/max, no thinking field:
    // https://docs.api.nvidia.com/nim/re/reference/moonshotai-kimi-k3-infer
    // The API defaults to max when effort isn't sent; this preset declares
    // high explicitly, matching config.toml.
    codexChatReasoning: {
      supportsThinking: false,
      supportsEffort: true,
      thinkingParam: "none",
      effortParam: "reasoning_effort",
      effortValueMode: "passthrough",
      outputFormat: "reasoning_content",
    },
    category: "aggregator",
    icon: "nvidia",
    iconColor: "#000000",
  },
  {
    name: "OpenCode Go",
    websiteUrl: "https://opencode.ai/go",
    apiKeyUrl: "https://opencode.ai/go",
    auth: generateThirdPartyAuth(""),
    config: generateThirdPartyConfig(
      "opencode_go",
      "https://opencode.ai/zen/go/v1",
      "glm-5.3",
    ),
    endpointCandidates: ["https://opencode.ai/zen/go/v1"],
    apiFormat: "openai_chat",
    // OpenCode Zen gateway: uniformly accepts a top-level reasoning_effort
    // (the same parameter its own client uses), but the legal tiers vary
    // per model (see each entry's reasoningLevels, mirroring models.dev;
    // the opencode client is equally strict about only sending values the
    // model declares) -- the proxy conversion layer clamps against the
    // table, and models with no declared effort don't get the field sent.
    // No vendor-native thinking field is sent.
    codexChatReasoning: {
      supportsThinking: true,
      supportsEffort: true,
      thinkingParam: "none",
      effortParam: "reasoning_effort",
      effortValueMode: "zen",
      outputFormat: "reasoning_content",
    },
    modelCatalog: modelCatalog([
      // https://opencode.ai/docs/go/ confirms the following new models all
      // go through Chat; context window/modalities/tiers are synced with
      // their official dependency https://models.dev/api.json (2026-09-10).
      {
        model: "glm-5.3",
        displayName: "GLM 5.3",
        contextWindow: 1000000,
        inputModalities: ["text"],
        reasoningLevels: ["low", "high", "max"],
        defaultReasoningLevel: "high",
      },
      {
        model: "glm-5.3-flash",
        displayName: "GLM 5.3 Flash",
        contextWindow: 1000000,
        inputModalities: ["text", "image"],
        reasoningLevels: ["low", "high", "max"],
        defaultReasoningLevel: "high",
      },
      {
        model: "kimi-k3",
        displayName: "Kimi K3",
        contextWindow: 1048576,
        inputModalities: ["text", "image"],
        reasoningLevels: ["max"],
      },
      {
        model: "deepseek-v4-pro",
        displayName: "DeepSeek V4 Pro",
        contextWindow: 1048576,
        reasoningLevels: ["high", "max"],
      },
      {
        model: "deepseek-v4-flash",
        displayName: "DeepSeek V4 Flash",
        contextWindow: 1048576,
        reasoningLevels: ["low", "high", "max"],
      },
      {
        model: "mimo-v2.5-pro",
        displayName: "MiMo V2.5 Pro",
        contextWindow: 1048576,
      },
    ]),
    category: "third_party",
    icon: "opencode",
    iconColor: "#211E1E",
  },
  {
    name: "AiHubMix",
    websiteUrl: "https://aihubmix.com",
    category: "aggregator",
    auth: generateThirdPartyAuth(""),
    config: generateThirdPartyConfig(
      "aihubmix",
      "https://aihubmix.com/v1",
      "gpt-5.6-sol",
    ),
    endpointCandidates: [
      "https://aihubmix.com/v1",
      "https://api.aihubmix.com/v1",
    ],
    icon: "aihubmix",
    iconColor: "#006FFB",
  },
  {
    name: "CherryIN",
    websiteUrl: "https://open.cherryin.ai",
    apiKeyUrl: "https://open.cherryin.ai/console/token",
    auth: generateThirdPartyAuth(""),
    config: generateThirdPartyConfig(
      "cherryin",
      "https://open.cherryin.net/v1",
      "openai/gpt-5.6-sol",
    ),
    endpointCandidates: ["https://open.cherryin.net/v1"],
    category: "aggregator",
    icon: "cherryin",
  },
  {
    name: "RelaxyCode",
    websiteUrl: "https://www.relaxycode.com",
    apiKeyUrl: "https://www.relaxycode.com/register",
    category: "third_party",
    auth: generateThirdPartyAuth(""),
    config: generateThirdPartyConfig(
      "relaxycode",
      "https://www.relaxycode.com/v1",
      "gpt-5.6-sol",
    ),
    icon: "relaxcode",
  },
  {
    name: "E-FlowCode",
    websiteUrl: "https://e-flowcode.cc",
    apiKeyUrl: "https://e-flowcode.cc",
    auth: {
      OPENAI_API_KEY: "",
    },
    config: `model_provider = "custom"
model = "gpt-5.6-sol"
model_reasoning_effort = "high"
disable_response_storage = true
personality = "pragmatic"

[model_providers.custom]
name = "E-FlowCode"
base_url = "https://e-flowcode.cc/v1"
wire_api = "responses"
requires_openai_auth = true
model_context_window = 1000000
model_auto_compact_token_limit = 9000000`,
    category: "third_party",
    endpointCandidates: ["https://e-flowcode.cc/v1"],
    icon: "eflowcode",
    iconColor: "#000000",
  },
  {
    name: "PIPELLM",
    websiteUrl: "https://code.pipellm.ai",
    apiKeyUrl: "https://code.pipellm.ai/login",
    auth: {
      OPENAI_API_KEY: "",
    },
    config: `model_provider = "custom"
model = "gpt-5.6-sol"
model_reasoning_effort = "medium"
disable_response_storage = true

[model_providers.custom]
name = "PIPELLM"
wire_api = "responses"
requires_openai_auth = true
base_url = "https://cc-api.pipellm.ai/v1"`,
    category: "aggregator",
    endpointCandidates: ["https://cc-api.pipellm.ai/v1"],
    icon: "pipellm",
  },
  {
    name: "OpenRouter",
    websiteUrl: "https://openrouter.ai",
    apiKeyUrl: "https://openrouter.ai/keys",
    auth: generateThirdPartyAuth(""),
    config: generateThirdPartyConfig(
      "openrouter",
      "https://openrouter.ai/api/v1",
      "gpt-5.6-sol",
    ),
    category: "aggregator",
    icon: "openrouter",
    iconColor: "#6566F1",
  },
  {
    name: "TheRouter",
    websiteUrl: "https://therouter.ai",
    apiKeyUrl: "https://dashboard.therouter.ai",
    auth: generateThirdPartyAuth(""),
    config: generateThirdPartyConfig(
      "therouter",
      "https://api.therouter.ai/v1",
      "openai/gpt-5.3-codex",
    ),
    endpointCandidates: ["https://api.therouter.ai/v1"],
    category: "aggregator",
  },
  {
    name: "JieKou AI",
    websiteUrl: "https://jiekou.ai/#model-library",
    apiKeyUrl: "https://jiekou.ai/settings/key-management",
    auth: generateThirdPartyAuth(""),
    config: generateThirdPartyConfig(
      "jiekou",
      "https://api.jiekou.ai/openai/v1",
      "claude-fable-5",
    ),
    endpointCandidates: ["https://api.jiekou.ai/openai/v1"],
    apiFormat: "openai_chat",
    modelCatalog: modelCatalog([
      {
        model: "claude-fable-5",
        displayName: "Claude Fable 5",
        contextWindow: 1000000,
        inputModalities: ["text", "image"],
      },
    ]),
    category: "aggregator",
    icon: "jiekou",
    iconColor: "#000000",
  },
  {
    name: "AICodeWith",
    websiteUrl: "https://aicodewith.ai",
    apiKeyUrl: "https://aicodewith.ai/login?tab=register",
    auth: generateThirdPartyAuth(""),
    // Official Codex-dedicated endpoint; the generic /v1 won't work (different protocol)
    config: generateThirdPartyConfig(
      "aicodewith",
      "https://api.aicodewith.ai/chatgpt/v1",
      "gpt-5.6-sol",
    ),
    endpointCandidates: ["https://api.aicodewith.ai/chatgpt/v1"],
    category: "aggregator",
    icon: "aicodewith",
    iconColor: "#3A3B40",
  },
];
