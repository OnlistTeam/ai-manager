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
  // api.kimi.com/coding is Kimi Code's China host; api.kimi.ai/coding, the
  // international one, is added below as plain "Kimi Code".
  ["Kimi For Coding", { kind: "vendor", name: "Kimi Code (China)" }],
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
/*
 * Output caps: where a vendor documents none, 32768 stands in. It only bounds
 * one reply, and a cap the model cannot reach would be worse than a low one.
 */
const UNDOCUMENTED_MAX_OUTPUT = 32768;

const KIMI_MODELS = [
  { id: "kimi-k2.7-code", name: "Kimi K2.7 Code", context: 262144 },
  { id: "kimi-k3", name: "Kimi K3", context: 1048576 },
];
const KIMI_CODE_MODELS = [
  { id: "kimi-for-coding", name: "Kimi for Coding", context: 1048576 },
  { id: "k3", name: "K3", context: 1048576 },
];
const STEPFUN_MODELS = [
  {
    id: "step-5-preview",
    name: "Step 5 Preview",
    context: 1000000,
    maxOutput: 64000,
  },
  { id: "step-3.7-flash", name: "Step 3.7 Flash", context: 256000 },
];
const QWEN_MODELS = [
  { id: "qwen3.7-plus", name: "Qwen3.7-Plus", context: 1000000 },
  { id: "qwen3.8-flash", name: "Qwen3.8-Flash", context: 1000000 },
];
// OpenCode serves each model over exactly one protocol; these are the ones it
// serves over chat completions.
const OPENCODE_CHAT_MODELS = [
  { id: "kimi-k3", name: "Kimi K3", context: 1048576, maxOutput: 131072 },
  { id: "glm-5.3", name: "GLM-5.3", context: 1000000, maxOutput: 131072 },
];

const ADDED_SERVICES = [
  {
    id: "kimi",
    name: "Kimi",
    kind: "vendor",
    website: "https://platform.kimi.ai",
    keys: "https://platform.kimi.ai",
    anthropic: "https://api.moonshot.ai/anthropic",
    // Moonshot's Responses endpoint serves kimi-k3 only.
    responses: "https://api.moonshot.ai/v1",
    chat: "https://api.moonshot.ai/v1",
    claudeModel: "kimi-k2.7-code",
    codexModel: "kimi-k3",
    models: KIMI_MODELS,
  },
  {
    id: "kimi-code",
    name: "Kimi Code",
    kind: "vendor",
    website: "https://www.kimi.ai",
    keys: "https://www.kimi.ai",
    anthropic: "https://api.kimi.ai/coding",
    responses: "https://api.kimi.ai/coding/v1",
    chat: "https://api.kimi.ai/coding/v1",
    claudeModel: "kimi-for-coding",
    codexModel: "kimi-for-coding",
    models: KIMI_CODE_MODELS,
  },
  // Pay-as-you-go. A Step Plan key is the same key; the /step_plan addresses
  // only change which balance is billed, and are a custom entry away.
  {
    id: "stepfun",
    name: "StepFun",
    kind: "vendor",
    website: "https://platform.stepfun.ai",
    keys: "https://platform.stepfun.ai",
    anthropic: "https://api.stepfun.ai",
    responses: "https://api.stepfun.ai/v1",
    chat: "https://api.stepfun.ai/v1",
    claudeModel: "step-5-preview",
    codexModel: "step-5-preview",
    models: STEPFUN_MODELS,
  },
  {
    id: "stepfun-cn",
    name: "StepFun (China)",
    kind: "vendor",
    website: "https://platform.stepfun.com",
    keys: "https://platform.stepfun.com",
    anthropic: "https://api.stepfun.com",
    responses: "https://api.stepfun.com/v1",
    chat: "https://api.stepfun.com/v1",
    claudeModel: "step-5-preview",
    codexModel: "step-5-preview",
    models: STEPFUN_MODELS,
  },
  // Alibaba's own Model Studio (DashScope), pay-as-you-go. Its Coding Plan
  // keys (`sk-sp-`) work only on the plan's own hosts.
  {
    id: "qwen",
    name: "Qwen",
    kind: "vendor",
    website: "https://modelstudio.console.alibabacloud.com",
    keys: "https://modelstudio.console.alibabacloud.com",
    anthropic: "https://dashscope-intl.aliyuncs.com/apps/anthropic",
    responses: "https://dashscope-intl.aliyuncs.com/compatible-mode/v1",
    chat: "https://dashscope-intl.aliyuncs.com/compatible-mode/v1",
    claudeModel: "qwen3.7-plus",
    codexModel: "qwen3.7-plus",
    models: QWEN_MODELS,
  },
  {
    id: "qwen-cn",
    name: "Qwen (China)",
    kind: "vendor",
    website: "https://bailian.console.aliyun.com",
    keys: "https://bailian.console.aliyun.com",
    anthropic: "https://dashscope.aliyuncs.com/apps/anthropic",
    responses: "https://dashscope.aliyuncs.com/compatible-mode/v1",
    chat: "https://dashscope.aliyuncs.com/compatible-mode/v1",
    claudeModel: "qwen3.7-plus",
    codexModel: "qwen3.7-plus",
    models: QWEN_MODELS,
  },
  // Chat completions only: /v1/messages answers but is undocumented, and
  // there is no Responses API.
  {
    id: "mistral",
    name: "Mistral",
    kind: "vendor",
    website: "https://mistral.ai",
    keys: "https://console.mistral.ai",
    chat: "https://api.mistral.ai/v1",
    models: [
      { id: "mistral-medium-3-5", name: "Mistral Medium 3.5", context: 262144 },
      { id: "mistral-small-2603", name: "Mistral Small 4", context: 262144 },
    ],
  },
  // No Anthropic endpoint; Responses is documented as beta.
  {
    id: "groq",
    name: "Groq",
    kind: "vendor",
    website: "https://groq.com",
    keys: "https://console.groq.com",
    responses: "https://api.groq.com/openai/v1",
    chat: "https://api.groq.com/openai/v1",
    codexModel: "openai/gpt-oss-120b",
    models: [
      {
        id: "openai/gpt-oss-120b",
        name: "GPT OSS 120B",
        context: 131072,
        maxOutput: 65536,
        input: ["text"],
      },
      {
        id: "openai/gpt-oss-20b",
        name: "GPT OSS 20B",
        context: 131072,
        maxOutput: 65536,
        input: ["text"],
      },
    ],
  },
  // Ollama's hosted models, with a key. Its Anthropic endpoint takes only a
  // bearer token, which is the slot Claude Code's preset writes. Codex is left
  // out: the cloud's Responses API is stateless and does not replay the
  // freeform tool calls Codex edits files with.
  {
    id: "ollama-cloud",
    name: "Ollama Cloud",
    kind: "vendor",
    website: "https://ollama.com",
    keys: "https://ollama.com",
    anthropic: "https://ollama.com",
    chat: "https://ollama.com/v1",
    claudeModel: "glm-5.3",
    claudeFastModel: "glm-5.3-flash",
    models: [
      { id: "glm-5.3", name: "GLM-5.3", context: 1048576, input: ["text"] },
      { id: "kimi-k2.7-code", name: "Kimi K2.7 Code", context: 262144 },
    ],
  },
  // Upstream carries xAI for Codex and Grok Build; this adds the chat tools.
  // Its Anthropic compatibility is documented as deprecated, so Claude Code
  // gets none.
  {
    id: "xai",
    name: "xAI (Grok)",
    kind: "vendor",
    website: "https://x.ai",
    keys: "https://console.x.ai",
    chat: "https://api.x.ai/v1",
    models: [
      { id: "grok-4.7", name: "Grok 4.7", context: 500000 },
      { id: "grok-build-0.1", name: "Grok Build 0.1", context: 256000 },
    ],
  },
  // OpenCode's paid gateway. Claude Code gets its Claude models under their
  // own ids, so like onList it names none and keeps the tool's own tiers.
  {
    id: "opencode-zen",
    name: "OpenCode Zen",
    kind: "relay",
    website: "https://opencode.ai",
    keys: "https://opencode.ai",
    anthropic: "https://opencode.ai/zen",
    responses: "https://opencode.ai/zen/v1",
    chat: "https://opencode.ai/zen/v1",
    codexModel: "gpt-5.6-sol",
    models: OPENCODE_CHAT_MODELS,
  },
  // The $10 subscription for open models. It serves no Claude model, so
  // Claude Code is pointed at one it does serve over Anthropic's protocol.
  {
    id: "opencode-go",
    name: "OpenCode Go",
    kind: "relay",
    website: "https://opencode.ai",
    keys: "https://opencode.ai",
    anthropic: "https://opencode.ai/zen/go",
    responses: "https://opencode.ai/zen/go/v1",
    chat: "https://opencode.ai/zen/go/v1",
    claudeModel: "minimax-m3",
    claudeFastModel: "qwen3.8-flash",
    codexModel: "gpt-6-luna",
    models: OPENCODE_CHAT_MODELS,
  },
];

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
          reasoning: true,
          input: model.input ?? ["text", "image"],
          contextWindow: model.context,
          maxTokens: model.maxOutput ?? UNDOCUMENTED_MAX_OUTPUT,
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
