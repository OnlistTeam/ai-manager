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
 * Matched against a preset's upstream `name`. `kind` is the group it shows
 * under on the add page (ADR-0057), and `name`, where given, is what the
 * catalogue ships instead of the upstream label.
 *
 * The renames exist because upstream names one region after the company and
 * suffixes the other: its "Kimi" and "MiniMax" are the China platforms and its
 * "MiniMax en" the international one. Side by side as cards that reads as a
 * duplicate, so each pair here is the plain name for the international
 * platform and "(China)" for the mainland one, which is how the companies
 * themselves split them.
 *
 * Two names that look like they belong here do not. `QwenCloud`
 * (qwencloud.com) and `千问AI平台` (qianwenai.com) are not Alibaba — Alibaba's
 * own platform is bailian.console.aliyun.com — so they are resellers trading on
 * the Qwen name, which is exactly what this list exists to keep out.
 *
 * Inference clouds that resell other labs' models (Together AI, Novita,
 * Nvidia) were dropped in ADR-0057: they are relays by another name, and the
 * relay group is kept to the few a user is likely to already have a key for.
 */
export const ALLOWED_SERVICES = new Map([
  // The labs, and the clouds that host them.
  ["Anthropic API", { kind: "vendor" }],
  ["OpenAI API", { kind: "vendor" }],
  ["Google Gemini API", { kind: "vendor" }],
  ["xAI (Grok)", { kind: "vendor" }],
  ["AWS Bedrock", { kind: "vendor" }],
  // Open-weight labs with their own platforms.
  ["DeepSeek", { kind: "vendor" }],
  ["Kimi", { kind: "vendor", name: "Kimi (China)" }],
  ["Kimi For Coding", { kind: "vendor", name: "Kimi Code" }],
  ["Zhipu GLM", { kind: "vendor" }],
  ["Zhipu GLM en", { kind: "vendor", name: "Z.ai" }],
  ["MiniMax", { kind: "vendor", name: "MiniMax (China)" }],
  ["MiniMax en", { kind: "vendor", name: "MiniMax" }],
  ["Nous Research", { kind: "vendor" }],
  // An aggregator, kept because it is a public company with a public catalogue
  // and a name it has to defend.
  ["OpenRouter", { kind: "relay" }],
]);

export function allowedService(serviceName) {
  return ALLOWED_SERVICES.get(String(serviceName ?? "").trim()) ?? null;
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
 * Claude Code's preset names no model. onList also answers to Claude Code's
 * own model ids, so the tool keeps its own default and its opus, sonnet and
 * haiku tiers, and the catalogue of some four hundred models stays one pick
 * away on Home. A model named here would have been written into all four
 * slots, putting every tier, background work included, on that one model.
 *
 * Two tools take the origin rather than `/v1`, because they append the version
 * themselves: Claude Code sends `/v1/messages` and Gemini CLI `/v1beta/…`.
 */
const OPUS = "anthropic/claude-opus-5";
const SONNET = "anthropic/claude-sonnet-5";

export const HOUSE_PRESETS = [
  {
    tool: "claude-code",
    id: "onlist",
    serviceName: "onList",
    defaultName: "onList",
    defaultModel: null,
    websiteUrl: ONLIST_SITE,
    apiKeyUrl: ONLIST_SITE,
    official: false,
    default: false,
    kind: "relay",
    settingsConfig: {
      env: {
        // Claude Code appends `/v1/messages`, so it gets the origin.
        ANTHROPIC_BASE_URL: ONLIST_ORIGIN,
        ANTHROPIC_AUTH_TOKEN: "",
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
    kind: "relay",
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
    kind: "relay",
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
    kind: "relay",
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
    kind: "relay",
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
    kind: "relay",
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
    kind: "relay",
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
    kind: "relay",
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

/*
 * Services upstream does not carry, or carries for only some tools (ADR-0057).
 *
 * Each entry names its endpoints by protocol, because a preset is written
 * straight into the tool's own config and nothing translates between them: a
 * tool gets one of these only where the service speaks that tool's protocol.
 *
 *   Claude Code                    `anthropic`, the origin; it appends /v1/messages
 *   Codex                          `responses`, ending in /v1; it appends /responses
 *   OpenCode, OpenClaw, Hermes, Pi `chat`, ending in /v1; they append /chat/completions
 *
 * Gemini CLI speaks only Google's own API, so it gets none of these, and Grok
 * Build is left to xAI and the relays it already has.
 *
 * Where upstream already ships the same service for a tool, its preset wins
 * and the one built here is dropped, so an entry can list every protocol the
 * service has without caring which tools upstream happens to cover.
 *
 * Every address and model id below was checked against the vendor's own docs,
 * and each address answered 401 or 403 without a key rather than 404
 * (2026-09-27). `models[0]` is the default; the chat tools also get the rest.
 */
const ADDED_SERVICES = [];

function claudeEnv(service) {
  const model = service.claudeModel ?? null;
  return {
    ANTHROPIC_BASE_URL: service.anthropic,
    ANTHROPIC_AUTH_TOKEN: "",
    ...(model
      ? {
          ANTHROPIC_MODEL: model,
          ANTHROPIC_DEFAULT_HAIKU_MODEL: service.claudeFastModel ?? model,
          ANTHROPIC_DEFAULT_SONNET_MODEL: model,
          ANTHROPIC_DEFAULT_OPUS_MODEL: model,
        }
      : {}),
  };
}

function codexConfig(service, model) {
  return [
    'model_provider = "custom"',
    `model = ${JSON.stringify(model)}`,
    'model_reasoning_effort = "high"',
    "disable_response_storage = true",
    "",
    "[model_providers.custom]",
    `name = ${JSON.stringify(service.name)}`,
    `base_url = ${JSON.stringify(service.responses)}`,
    'wire_api = "responses"',
    "requires_openai_auth = true",
    "",
  ].join("\n");
}

const ADDED_BUILDERS = {
  "claude-code": (service) =>
    service.anthropic && [service.claudeModel ?? null, { env: claudeEnv(service) }],
  codex: (service) =>
    service.responses &&
    service.codexModel && [
      service.codexModel,
      {
        auth: { OPENAI_API_KEY: "" },
        config: codexConfig(service, service.codexModel),
      },
    ],
  opencode: (service) =>
    service.chat &&
    service.models?.length && [
      service.models[0].id,
      {
        npm: "@ai-sdk/openai-compatible",
        name: service.name,
        options: { baseURL: service.chat, apiKey: "" },
        models: Object.fromEntries(
          service.models.map((model) => [model.id, { name: model.name }]),
        ),
      },
    ],
  openclaw: (service) =>
    service.chat &&
    service.models?.length && [
      service.models[0].id,
      {
        baseUrl: service.chat,
        apiKey: "",
        api: "openai-completions",
        models: service.models.map((model) => ({
          id: model.id,
          name: model.name,
          contextWindow: model.context,
        })),
      },
    ],
  hermes: (service) =>
    service.chat &&
    service.models?.length && [
      service.models[0].id,
      {
        name: service.id,
        base_url: service.chat,
        api_key: "",
        api_mode: "chat_completions",
        models: service.models.map((model) => ({
          id: model.id,
          name: model.name,
          context_length: model.context,
        })),
      },
    ],
  pi: (service) =>
    service.chat &&
    service.models?.length && [
      service.models[0].id,
      {
        name: service.name,
        baseUrl: service.chat,
        api: "openai-completions",
        apiKey: "",
        models: service.models.map((model) => ({
          id: model.id,
          name: model.name,
          reasoning: model.reasoning,
          input: model.input,
          contextWindow: model.context,
          maxTokens: model.maxOutput,
        })),
      },
    ],
};

export const ADDED_PRESETS = ADDED_SERVICES.flatMap((service) =>
  Object.entries(ADDED_BUILDERS).flatMap(([tool, build]) => {
    const built = build(service);
    if (!built) return [];
    const [defaultModel, settingsConfig] = built;
    return [
      {
        tool,
        id: service.id,
        serviceName: service.name,
        defaultName: service.name,
        defaultModel,
        websiteUrl: service.website,
        apiKeyUrl: service.keys,
        official: false,
        default: false,
        kind: service.kind,
        settingsConfig,
      },
    ];
  }),
);
