/**
 * Which services this product ships a preset for, and the one it runs itself.
 *
 * The upstream catalogue lists 95 services, and half of them are third-party
 * key resellers: relays that buy capacity somewhere and sell access to it under
 * their own name. Shipping a preset for one of those is a quiet endorsement —
 * the user pastes their key into an address this application suggested — and it
 * is an endorsement nothing here can back. The picker even told them the list
 * was "vetted", which was never true of 39 hosts nobody here has audited.
 *
 * So the rule is: a preset exists for a service that is internationally known
 * and answerable for itself. In practice that means the company that built the
 * model or the cloud that hosts it, plus OpenRouter, which is an aggregator but
 * a public, accountable one.
 *
 * This is a policy rather than a one-time deletion on purpose. The preset
 * sources are upstream CC Switch files that get cherry-picked (ADR-0001), so a
 * relay added upstream tomorrow would walk straight back in. Filtering at the
 * boundary means the answer holds without anyone having to remember it, and the
 * criterion sits in one place where it can be argued with.
 *
 * Nobody is blocked by an absence: "Custom endpoint" takes any address, which
 * is the honest place for a service this project cannot vouch for.
 */

/**
 * Matched against a preset's `name`, which the catalogue projects verbatim as
 * `serviceName`.
 *
 * Two names that look like they belong here do not. `QwenCloud`
 * (qwencloud.com) and `千问AI平台` (qianwenai.com) are not Alibaba — Alibaba's
 * own platform is bailian.console.aliyun.com — so they are resellers trading on
 * the Qwen name, which is exactly what this list exists to keep out.
 */
export const ALLOWED_SERVICES = new Set([
  // The labs, and the clouds that host them.
  "Anthropic API",
  "OpenAI API",
  "Google Gemini API",
  "xAI (Grok)",
  "AWS Bedrock",
  // Open-weight labs with their own international platforms.
  "DeepSeek",
  "Kimi",
  "Kimi For Coding",
  "Zhipu GLM",
  "Zhipu GLM en",
  "MiniMax",
  "MiniMax en",
  "Nous Research",
  // International inference clouds.
  "Together AI",
  "Nvidia",
  "Novita AI",
  // An aggregator, kept because it is a public company with a public catalogue
  // and a name it has to defend.
  "OpenRouter",
]);

export function isAllowedService(serviceName) {
  return ALLOWED_SERVICES.has(String(serviceName ?? "").trim());
}

/** The address every onList preset points at. */
const ONLIST_ORIGIN = "https://onlist.io";
const ONLIST_V1 = `${ONLIST_ORIGIN}/v1`;
const ONLIST_SITE = "https://onlist.io";

/*
 * onList speaks the OpenRouter dialect — prefixed model ids and the same four
 * routes — so each of these mirrors the shape of that tool's OpenRouter preset
 * rather than inventing one. Every model id below was checked against
 * https://onlist.io/v1/models.
 *
 * Two tools take the origin rather than `/v1`, because they append the version
 * themselves: Claude Code sends `/v1/messages` and Gemini CLI `/v1beta/…`.
 */
const OPUS = "anthropic/claude-opus-5";
const SONNET = "anthropic/claude-sonnet-5";
const HAIKU = "anthropic/claude-haiku-4.5";

export const HOUSE_PRESETS = [
  {
    tool: "claude-code",
    id: "onlist",
    serviceName: "onList",
    defaultName: "onList",
    defaultModel: SONNET,
    websiteUrl: ONLIST_SITE,
    apiKeyUrl: ONLIST_SITE,
    official: false,
    default: false,
    settingsConfig: {
      env: {
        // Claude Code appends `/v1/messages`, so it gets the origin.
        ANTHROPIC_BASE_URL: ONLIST_ORIGIN,
        ANTHROPIC_AUTH_TOKEN: "",
        ANTHROPIC_MODEL: SONNET,
        ANTHROPIC_DEFAULT_HAIKU_MODEL: HAIKU,
        ANTHROPIC_DEFAULT_SONNET_MODEL: SONNET,
        ANTHROPIC_DEFAULT_OPUS_MODEL: OPUS,
      },
    },
  },
  {
    tool: "codex",
    id: "onlist",
    serviceName: "onList",
    defaultName: "onList",
    defaultModel: "openai/gpt-5.6-sol",
    websiteUrl: ONLIST_SITE,
    apiKeyUrl: ONLIST_SITE,
    official: false,
    default: false,
    settingsConfig: {
      auth: { OPENAI_API_KEY: "" },
      config: [
        'model_provider = "custom"',
        'model = "openai/gpt-5.6-sol"',
        'model_reasoning_effort = "high"',
        "disable_response_storage = true",
        "",
        "[model_providers.custom]",
        'name = "onList"',
        // Codex concatenates `{base_url}/responses`, so the version segment
        // has to be here (ADR-0041 amendment).
        `base_url = "${ONLIST_V1}"`,
        'wire_api = "responses"',
        "requires_openai_auth = true",
        "",
      ].join("\n"),
    },
  },
  {
    tool: "gemini-cli",
    id: "onlist",
    serviceName: "onList",
    defaultName: "onList",
    defaultModel: "google/gemini-3.6-flash",
    websiteUrl: ONLIST_SITE,
    apiKeyUrl: ONLIST_SITE,
    official: false,
    default: false,
    settingsConfig: {
      env: {
        // Gemini CLI appends `/v1beta/models/…`, so it gets the origin.
        GOOGLE_GEMINI_BASE_URL: ONLIST_ORIGIN,
        GEMINI_API_KEY: "",
      },
      config: {},
    },
  },
  {
    tool: "opencode",
    id: "onlist",
    serviceName: "onList",
    defaultName: "onList",
    defaultModel: SONNET,
    websiteUrl: ONLIST_SITE,
    apiKeyUrl: ONLIST_SITE,
    official: false,
    default: false,
    settingsConfig: {
      npm: "@ai-sdk/anthropic",
      name: "onList",
      options: { baseURL: ONLIST_V1, apiKey: "", setCacheKey: true },
      models: {
        [SONNET]: { name: "Claude Sonnet 5" },
        [OPUS]: { name: "Claude Opus 5" },
      },
    },
  },
  {
    tool: "grok-build",
    id: "onlist",
    serviceName: "onList",
    defaultName: "onList",
    defaultModel: "x-ai/grok-4.5",
    websiteUrl: ONLIST_SITE,
    apiKeyUrl: ONLIST_SITE,
    official: false,
    default: false,
    settingsConfig: {
      config: [
        "[models]",
        'default = "x-ai/grok-4.5"',
        "",
        '[model."x-ai/grok-4.5"]',
        'model = "x-ai/grok-4.5"',
        `base_url = "${ONLIST_V1}"`,
        'name = "onList"',
        'api_key = ""',
        'api_backend = "responses"',
        "context_window = 500000",
        "",
      ].join("\n"),
    },
  },
  {
    tool: "openclaw",
    id: "onlist",
    serviceName: "onList",
    defaultName: "onList",
    defaultModel: OPUS,
    websiteUrl: ONLIST_SITE,
    apiKeyUrl: ONLIST_SITE,
    official: false,
    // Pi and OpenClaw have no first-party vendor to default to.
    default: true,
    settingsConfig: {
      baseUrl: ONLIST_V1,
      apiKey: "",
      api: "openai-completions",
      models: [
        { id: OPUS, name: "Claude Opus 5", contextWindow: 1000000 },
        { id: SONNET, name: "Claude Sonnet 5", contextWindow: 1000000 },
      ],
    },
  },
  {
    tool: "hermes",
    id: "onlist",
    serviceName: "onList",
    defaultName: "onList",
    defaultModel: OPUS,
    websiteUrl: ONLIST_SITE,
    apiKeyUrl: ONLIST_SITE,
    official: false,
    default: false,
    settingsConfig: {
      name: "onlist",
      base_url: ONLIST_V1,
      api_key: "",
      api_mode: "chat_completions",
      models: [
        { id: OPUS, name: "Claude Opus 5", context_length: 1000000 },
        { id: SONNET, name: "Claude Sonnet 5", context_length: 1000000 },
      ],
    },
  },
  {
    tool: "pi",
    id: "onlist",
    serviceName: "onList",
    defaultName: "onList",
    defaultModel: SONNET,
    websiteUrl: ONLIST_SITE,
    apiKeyUrl: ONLIST_SITE,
    official: false,
    // Pi and OpenClaw have no first-party vendor to default to.
    default: true,
    settingsConfig: {
      name: "onList",
      // Pi speaks the Anthropic Messages API and appends `/v1/messages`.
      baseUrl: ONLIST_ORIGIN,
      api: "anthropic-messages",
      apiKey: "",
      models: [
        {
          id: SONNET,
          name: "Claude Sonnet 5",
          reasoning: true,
          input: ["text", "image"],
          contextWindow: 1000000,
          maxTokens: 128000,
        },
        {
          id: OPUS,
          name: "Claude Opus 5",
          reasoning: true,
          input: ["text", "image"],
          contextWindow: 1000000,
          maxTokens: 128000,
        },
      ],
    },
  },
];
