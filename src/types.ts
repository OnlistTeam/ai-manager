export type ProviderCategory =
  | "official" // official
  | "cn_official" // open-source official (formerly "domestic official")
  | "cloud_provider" // cloud provider (AWS Bedrock, etc.)
  | "aggregator" // aggregator site
  | "third_party" // third-party provider
  | "custom" // custom
  | "omo" // Oh My OpenCode
  | "omo-slim"; // Oh My OpenCode Slim

export interface Provider {
  id: string;
  name: string;
  settingsConfig: Record<string, unknown>; // App config object: settings.json for Claude; { auth, config } for Codex
  websiteUrl?: string;
  // Added: provider category (used for differentiated hints / capability toggles)
  category?: ProviderCategory;
  createdAt?: number; // Creation timestamp (milliseconds)
  sortIndex?: number; // Sort index (for custom drag-and-drop ordering)
  // Notes
  notes?: string;
  // Optional: provider metadata (stored only in ~/.cc-switch/config.json, never written to the live config)
  meta?: ProviderMeta;
  // Icon config
  icon?: string; // Icon name (e.g. "openai", "anthropic")
  iconColor?: string; // Icon color (hex format, e.g. "#00A67E")
  // Whether this is included in the failover queue
  inFailoverQueue?: boolean;
}

export interface AppConfig {
  providers: Record<string, Provider>;
  current: string;
}

// Custom endpoint config
export interface CustomEndpoint {
  url: string;
  addedAt: number;
  lastUsed?: number;
}

// Endpoint candidate (used by the endpoint speed-test dialog)
export interface EndpointCandidate {
  id?: string;
  url: string;
  isCustom?: boolean;
}

import type { TemplateType } from "./config/constants";

// Usage query script config
export interface UsageScript {
  enabled: boolean; // Whether usage query is enabled
  language: "javascript"; // Script language
  code: string; // Script code (JSON-formatted config)
  timeout?: number; // Timeout (seconds, default 10)
  templateType?: TemplateType; // Template type (used by the backend to determine validation rules)
  apiKey?: string; // API key dedicated to usage queries (used by the generic template)
  baseUrl?: string; // Base URL dedicated to usage queries (used by the generic and NewAPI templates)
  accessToken?: string; // Access token (used by the NewAPI template)
  userId?: string; // User ID (used by the NewAPI template)
  accessKeyId?: string; // Volcano Engine (Volcengine) AccessKey ID (used to sign usage queries, separate from the inference key)
  secretAccessKey?: string; // Volcano Engine (Volcengine) SecretAccessKey
  teamOrganizationId?: string; // Zhipu team-plan organization ID (bigmodel-organization request header)
  teamProjectId?: string; // Zhipu team-plan project ID (bigmodel-project request header)
  codingPlanProvider?: string; // Coding Plan provider identifier (e.g. "kimi", "zhipu", "minimax")
  autoQueryInterval?: number; // Auto-query interval (minutes; 0 disables it)
  autoIntervalMinutes?: number; // Auto-query interval (minutes) - alias field
  request?: {
    // Request config
    url?: string; // Request URL
    method?: string; // HTTP method
    headers?: Record<string, string>; // Request headers
    body?: unknown; // Request body
  };
}

const DEFAULT_USAGE_SCRIPT: UsageScript = {
  enabled: false,
  language: "javascript",
  code: "",
  timeout: 10,
  autoQueryInterval: 5,
};

export function createUsageScript(
  overrides?: Partial<UsageScript>,
): UsageScript {
  return { ...DEFAULT_USAGE_SCRIPT, ...overrides };
}

// Usage data for a single plan
export interface UsageData {
  planName?: string; // Plan name (optional)
  extra?: string; // Extra field for freely adding text to display (optional)
  isValid?: boolean; // Whether the plan is valid (optional)
  invalidMessage?: string; // Reason the plan is invalid (optional, shown when isValid is false)
  total?: number; // Total quota (optional)
  used?: number; // Used quota (optional)
  remaining?: number; // Remaining quota (optional)
  unit?: string; // Unit (optional)
}

// Usage query result (supports multiple plans)
export interface UsageResult {
  success: boolean;
  data?: UsageData[]; // Changed to an array to support returning multiple plans
  error?: string;
}

export type AuthBindingSource = "provider_config" | "managed_account";

export interface AuthBinding {
  source: AuthBindingSource;
  authProvider?: string;
  accountId?: string;
}

export interface ClaudeDesktopModelRoute {
  model: string;
  labelOverride?: string;
  supports1m?: boolean;
}

export type CodexChatThinkingParam =
  | "none"
  | "thinking"
  | "enable_thinking"
  | "reasoning_split";

export type CodexChatEffortParam =
  | "none"
  | "reasoning_effort"
  // OpenRouter's native normalized object reasoning:{effort} (distinct from the top-level OpenAI alias reasoning_effort)
  | "reasoning.effort";

export type CodexChatEffortValueMode =
  | "passthrough"
  | "low_high"
  | "deepseek"
  // OpenRouter effort enum xhigh|high|medium|low|minimal (no max; max clamps to xhigh)
  | "openrouter"
  // OpenCode Zen gateway: valid tiers are per-model, see each modelCatalog entry's reasoningLevels
  // (mirrors models.dev); the proxy conversion layer clamps by looking up the requested model, and omits the effort field when there's no table
  | "zen";

export type CodexChatReasoningOutputFormat =
  | "auto"
  | "reasoning_content"
  | "reasoning"
  | "reasoning_details"
  | "think_tags";

export interface CodexChatReasoning {
  supportsThinking?: boolean;
  supportsEffort?: boolean;
  thinkingParam?: CodexChatThinkingParam;
  effortParam?: CodexChatEffortParam;
  effortValueMode?: CodexChatEffortValueMode;
  // Declarative field: marks where upstream returns reasoning. Extraction currently relies on enumerating fields and does not read this value yet (think_tags is not wired up).
  outputFormat?: CodexChatReasoningOutputFormat;
}

export type PromptCacheRoutingMode = "auto" | "enabled" | "disabled";

export interface LocalProxyRequestOverrides {
  headers?: Record<string, string>;
  body?: Record<string, unknown>;
}

// Provider metadata (field names match the backend, kept in snake_case)
export interface ProviderMeta {
  // Custom endpoints: keyed by URL, value is endpoint info
  custom_endpoints?: Record<string, CustomEndpoint>;
  // Whether to apply the common config fragment when switching/syncing to live
  commonConfigEnabled?: boolean;
  // Claude Desktop third-party config write mode
  claudeDesktopMode?: "direct" | "proxy";
  // Claude Desktop local routing mode: Claude-safe route -> upstream model
  claudeDesktopModelRoutes?: Record<string, ClaudeDesktopModelRoute>;
  // Usage query script config
  usage_script?: UsageScript;
  // Endpoint management: automatically select the best endpoint after speed testing
  endpointAutoSelect?: boolean;
  // Provider cost multiplier
  costMultiplier?: string;
  // Provider pricing-model source
  pricingModelSource?: string;
  // API format (used by Claude / Codex providers)
  // - "anthropic": native Anthropic Messages API format, passed through directly
  // - "openai_chat": OpenAI Chat Completions format, requires format conversion
  // - "openai_responses": OpenAI Responses API format, requires format conversion
  // - "gemini_native": Gemini Native generateContent API format, requires format conversion
  apiFormat?:
    | "anthropic"
    | "openai_chat"
    | "openai_responses"
    | "gemini_native";
  // Generic auth binding
  authBinding?: AuthBinding;
  // Claude auth field name
  apiKeyField?: ClaudeApiKeyField;
  // Whether to treat base_url as a complete API endpoint (the proxy uses this URL directly, without appending a path)
  isFullUrl?: boolean;
  // Prompt cache key for OpenAI Responses-compatible endpoints (improves cache hit rate)
  promptCacheKey?: string;
  // Session-based prompt-cache routing for Codex Responses -> Chat conversions.
  // auto enables only for known-compatible upstreams; enabled/disabled are user overrides.
  promptCacheRouting?: PromptCacheRoutingMode;
  // Codex OAuth FAST mode: injects service_tier="priority" on ChatGPT Codex requests
  codexFastMode?: boolean;
  // Codex Responses -> Chat Completions reasoning capability metadata
  codexChatReasoning?: CodexChatReasoning;
  // Codex → Anthropic path: emulate the Claude Code client (disabled by default; only an explicit true enables it)
  impersonateClaudeCode?: boolean;
  // Codex → Anthropic path: override the Anthropic max_tokens (output ceiling).
  // Codex does not forward model_max_output_tokens in the request body; without
  // this the path falls back to a conservative 8192 default, which can truncate
  // long/thinking-heavy responses. When set (>0) it takes precedence over the
  // request value and the default.
  maxOutputTokens?: number;
  // Custom User-Agent for local proxy routing. Only applied by the local proxy.
  customUserAgent?: string;
  // Local proxy request overrides. Only applied by the local proxy after route transforms.
  localProxyRequestOverrides?: LocalProxyRequestOverrides;
  // Whether this provider is currently projected into an additive app's live config.
  liveConfigManaged?: boolean;
  // Provider type (used to identify special providers such as Copilot)
  providerType?: string;
  // GitHub Copilot linked account ID (legacy field, kept for backward-compatible reads)
  githubAccountId?: string;
}

// Skill sync method
export type SkillSyncMethod = "auto" | "symlink" | "copy";

// Skill storage location
export type SkillStorageLocation = "cc_switch" | "unified";

// Claude API format type
// - "anthropic": native Anthropic Messages API format, passed through directly
// - "openai_chat": OpenAI Chat Completions format, requires format conversion
// - "openai_responses": OpenAI Responses API format, requires format conversion
// - "gemini_native": Gemini Native generateContent API format, requires format conversion
export type ClaudeApiFormat =
  | "anthropic"
  | "openai_chat"
  | "openai_responses"
  | "gemini_native";

// Codex API format type
// - "openai_responses": OpenAI Responses API format, passed through directly
// - "openai_chat": OpenAI Chat Completions format, requires local routing to convert
// - "anthropic": native Anthropic Messages format, needs local routing to convert to Responses
export type CodexApiFormat = "openai_responses" | "openai_chat" | "anthropic";

export interface CodexCatalogModel {
  model: string;
  displayName?: string;
  contextWindow?: string | number;
  // Hidden provider capability metadata for the generated model catalog.
  // supportsParallelToolCalls is native-profile-only; inputModalities wins over
  // automatic text-only model detection for every profile.
  supportsParallelToolCalls?: boolean;
  inputModalities?: string[];
  // Vendor's OFFICIAL base_instructions (model identity / system preamble).
  // Codex requires this field in every catalog entry; when omitted the backend
  // falls back to a neutral default. e.g. MiMo "developed by Xiaomi".
  baseInstructions?: string;
  // Per-model reasoning effort levels exposed in the generated Codex catalog
  // (e.g. ["none", "low", "medium", "high", "xhigh", "max"]). When omitted the
  // backend keeps the template's conservative none/high default.
  reasoningLevels?: string[];
  // Per-model default reasoning effort. Only meaningful together with
  // reasoningLevels; when omitted the backend keeps the template default if it
  // is still in the list, otherwise the highest declared level.
  defaultReasoningLevel?: string;
}

// Claude auth field type
export type ClaudeApiKeyField = "ANTHROPIC_AUTH_TOKEN" | "ANTHROPIC_API_KEY";

// App visibility config shown on the home page
export interface VisibleApps {
  claude: boolean;
  "claude-desktop": boolean;
  codex: boolean;
  gemini: boolean;
  grokbuild: boolean;
  opencode: boolean;
  openclaw: boolean;
  hermes: boolean;
  pi: boolean;
}

// WebDAV sync status
export interface WebDavSyncStatus {
  lastSyncAt?: number | null;
  lastError?: string | null;
  lastErrorSource?: string | null;
  lastRemoteEtag?: string | null;
  lastLocalManifestHash?: string | null;
  lastRemoteManifestHash?: string | null;
}

// WebDAV sync config
export interface WebDavSyncSettings {
  enabled?: boolean;
  autoSync?: boolean;
  baseUrl?: string;
  username?: string;
  password?: string;
  remoteRoot?: string;
  profile?: string;
  status?: WebDavSyncStatus;
}

// S3 sync config
export interface S3SyncSettings {
  enabled?: boolean;
  autoSync?: boolean;
  region?: string;
  bucket?: string;
  accessKeyId?: string;
  secretAccessKey?: string;
  endpoint?: string;
  remoteRoot?: string;
  profile?: string;
  status?: WebDavSyncStatus;
}

export type RemoteSnapshotLayout = "current" | "legacy";

// Remote snapshot info (preview before download)
export interface RemoteSnapshotInfo {
  deviceName: string;
  createdAt: string;
  snapshotId: string;
  version: number;
  protocolVersion: number;
  dbCompatVersion?: number | null;
  compatible: boolean;
  artifacts: string[];
  layout: RemoteSnapshotLayout;
  remotePath: string;
}

// App settings type (used by the settings dialog and the Tauri API)
// Stored locally at ~/.cc-switch/settings.json, not synced with the database
export interface Settings {
  // ===== Device-level UI settings =====
  // Whether to show the icon in the system tray (macOS menu bar)
  showInTray: boolean;
  // Whether clicking the close button minimizes to tray instead of closing the app
  minimizeToTrayOnClose: boolean;
  // Whether to enable app-level window control buttons (minimize/maximize/close)
  useAppWindowControls?: boolean;
  // Enable Claude plugin integration (writes primaryApiKey to ~/.claude/config.json)
  enableClaudePluginIntegration?: boolean;
  // Skip the Claude Code first-run onboarding confirmation (writes hasCompletedOnboarding to ~/.claude.json)
  skipClaudeOnboarding?: boolean;
  // Whether to launch on system startup
  launchOnStartup?: boolean;
  // Silent startup (don't show the main window on launch)
  silentStartup?: boolean;
  // Whether to enable the local proxy feature on the main page (off by default)
  enableLocalProxy?: boolean;
  // User has confirmed the local proxy first-run notice
  proxyConfirmed?: boolean;
  // User has confirmed the usage query first-run notice
  usageConfirmed?: boolean;
  usageDashboardRefreshIntervalMs?: number;
  // Whether to show the failover toggle independently on the main page
  enableFailoverToggle?: boolean;
  // Whether to show the project profile switcher on the main page header
  showProfileSwitcher?: boolean;
  // Preserve Codex ChatGPT login in auth.json when switching third-party providers
  preserveCodexOfficialAuthOnSwitch?: boolean;
  // Run official Codex under the shared "custom" provider id so future
  // sessions share one resume-history bucket with third-party providers
  unifyCodexSessionHistory?: boolean;
  // User opted in (enable dialog checkbox) to migrate existing official sessions
  unifyCodexMigrateExisting?: boolean;
  // User has confirmed the failover toggle first-run notice
  failoverConfirmed?: boolean;
  // User has confirmed the first-run welcome notice
  firstRunNoticeConfirmed?: boolean;
  // User has confirmed the auto-sync traffic warning
  autoSyncConfirmed?: boolean;
  // User has confirmed the common config first-run notice
  commonConfigConfirmed?: boolean;
  // Preferred language (optional, defaults to Chinese)
  language?: "en" | "zh" | "zh-TW" | "ja";

  // App visibility on the main page (all shown by default)
  visibleApps?: VisibleApps;

  // ===== Device-level directory overrides =====
  // Override the Claude Code config directory (optional)
  claudeConfigDir?: string;
  // Override the Codex config directory (optional)
  codexConfigDir?: string;
  // Override the Gemini config directory (optional)
  geminiConfigDir?: string;
  // Override the Grok Build config directory (optional)
  grokConfigDir?: string;
  // Override the OpenCode config directory (optional)
  opencodeConfigDir?: string;
  // Override the OpenClaw config directory (optional)
  openclawConfigDir?: string;
  // Override the Hermes config directory (optional)
  hermesConfigDir?: string;
  // Override the Pi agent config directory (optional)
  piConfigDir?: string;

  // ===== Current provider ID (device-level) =====
  // Current Claude provider ID (takes precedence over the database's is_current)
  currentProviderClaude?: string;
  // Current Claude Desktop provider ID (takes precedence over the database's is_current)
  currentProviderClaudeDesktop?: string;
  // Current Codex provider ID (takes precedence over the database's is_current)
  currentProviderCodex?: string;
  // Current Gemini provider ID (takes precedence over the database's is_current)
  currentProviderGemini?: string;

  // ===== Skill sync settings =====
  // Skill sync method: auto (default, prefers symlink), symlink, copy
  skillSyncMethod?: SkillSyncMethod;
  // Skill storage location: cc_switch (default) or unified (~/.agents/skills/)
  skillStorageLocation?: SkillStorageLocation;

  // ===== WebDAV v2 sync settings =====
  webdavSync?: WebDavSyncSettings;

  // ===== S3 sync settings =====
  s3Sync?: S3SyncSettings;

  // ===== Backup policy settings =====
  // Auto-backup interval in hours (0=disabled, default 24)
  backupIntervalHours?: number;
  // Maximum backup files to retain (default 10)
  backupRetainCount?: number;

  // ===== Terminal settings =====
  // Preferred terminal app (optional, defaults to the system default terminal)
  // macOS: "terminal" | "iterm2" | "warp" | "alacritty" | "kitty" | "ghostty" | "wezterm" | "kaku"
  // Windows: "cmd" | "powershell" | "wt"
  // Linux: "gnome-terminal" | "konsole" | "xfce4-terminal" | "alacritty" | "kitty" | "ghostty"
  preferredTerminal?: string;

  // ===== Local automatic migration status =====
  localMigrations?: {
    codexThirdPartyHistoryProviderBucketV1?: {
      completedAt: string;
      targetProviderId: string;
      sourceProviderIds?: string[];
      migratedJsonlFiles?: number;
      migratedStateRows?: number;
    };
  };
}

export interface SessionMeta {
  providerId: string;
  sessionId: string;
  title?: string;
  summary?: string;
  projectDir?: string | null;
  createdAt?: number;
  lastActiveAt?: number;
  sourcePath?: string;
  resumeCommand?: string;
}

export interface SessionMessage {
  role: string;
  content: string;
  ts?: number;
}

// MCP server connection spec (loose: allows extra fields)
export interface McpServerSpec {
  // Optional: community .mcp.json stdio configs commonly omit type
  type?: "stdio" | "http" | "sse";
  // stdio fields
  command?: string;
  args?: string[];
  env?: Record<string, string>;
  cwd?: string;
  // http and sse fields
  url?: string;
  headers?: Record<string, string>;
  // Generic fields
  [key: string]: unknown;
}

// v3.7.0: MCP server per-app enabled state
export interface McpApps {
  claude: boolean;
  "claude-desktop"?: boolean;
  codex: boolean;
  gemini: boolean;
  grokbuild?: boolean;
  opencode: boolean;
  openclaw: boolean;
  hermes: boolean;
}

// MCP server entry (v3.7.0 unified structure)
export interface McpServer {
  id: string;
  name: string;
  server: McpServerSpec;
  apps: McpApps; // v3.7.0: marks which clients the server is enabled for
  description?: string;
  tags?: string[];
  homepage?: string;
  docs?: string;
  // Legacy field, kept for compatibility (v3.6.x and earlier)
  enabled?: boolean; // Deprecated; v3.7.0 uses the apps field instead
  source?: string;
  [key: string]: unknown;
}

// MCP server map (id -> McpServer)
export type McpServersMap = Record<string, McpServer>;

// MCP config status
export interface McpStatus {
  userConfigPath: string;
  userConfigExists: boolean;
  serverCount: number;
}

// New: MCP list response from config.json
export interface McpConfigResponse {
  configPath: string;
  servers: Record<string, McpServer>;
}

// ============================================================================
// Universal Provider - config shared across apps
// ============================================================================

// Universal provider's per-app enabled state
export interface UniversalProviderApps {
  claude: boolean;
  codex: boolean;
  gemini: boolean;
}

// Claude model config
export interface ClaudeModelConfig {
  model?: string;
  haikuModel?: string;
  sonnetModel?: string;
  opusModel?: string;
}

// Codex model config
export interface CodexModelConfig {
  model?: string;
  reasoningEffort?: string;
}

// Gemini model config
export interface GeminiModelConfig {
  model?: string;
}

// Model config for each app
export interface UniversalProviderModels {
  claude?: ClaudeModelConfig;
  codex?: CodexModelConfig;
  gemini?: GeminiModelConfig;
}

// Universal provider (config shared across apps)
export interface UniversalProvider {
  id: string;
  name: string;
  providerType: string; // e.g. "newapi" | "custom"
  apps: UniversalProviderApps;
  baseUrl: string;
  apiKey: string;
  models: UniversalProviderModels;
  websiteUrl?: string;
  notes?: string;
  icon?: string;
  iconColor?: string;
  meta?: ProviderMeta;
  createdAt?: number;
  sortIndex?: number;
}

// Universal provider map (id -> UniversalProvider)
export type UniversalProvidersMap = Record<string, UniversalProvider>;

// ============================================================================
// OpenCode-specific config (v3.9.2+)
// ============================================================================

// OpenCode model config
export interface OpenCodeModel {
  name: string;
  limit?: {
    context?: number;
    output?: number;
  };
  options?: Record<string, unknown>; // Model-level extra options (provider routing, etc.)
  // Supports arbitrary extra fields (cost, modalities, thinking, variants, etc.)
  [key: string]: unknown;
}

// OpenCode provider options
export interface OpenCodeProviderOptions {
  baseURL?: string;
  apiKey?: string;
  headers?: Record<string, string>;
  // Supports extra options (timeout, setCacheKey, etc.)
  [key: string]: unknown;
}

// OpenCode provider config (settings_config structure)
export interface OpenCodeProviderConfig {
  npm: string; // AI SDK package name, e.g. "@ai-sdk/openai-compatible"
  name?: string; // Provider display name
  options: OpenCodeProviderOptions;
  models: Record<string, OpenCodeModel>;
}

// OpenCode MCP server config (differs from the unified format)
export interface OpenCodeMcpServerSpec {
  type: "local" | "remote";
  // local-type fields
  command?: string[]; // Differs from the unified format: command and args are merged into one array
  environment?: Record<string, string>; // Differs from the unified format: uses environment instead of env
  // remote-type fields
  url?: string;
  headers?: Record<string, string>;
  // Generic fields
  enabled?: boolean;
}

// ============================================================================
// OpenClaw-specific config (v3.11.0+)
// ============================================================================

// OpenClaw model config
export interface OpenClawModel {
  id: string;
  name: string;
  alias?: string;
  reasoning?: boolean; // Whether reasoning mode is supported (e.g. o1, DeepSeek R1)
  input?: string[]; // Supported input types (e.g. ["text"], ["text", "image"])
  cost?: {
    input: number;
    output: number;
    cacheRead?: number; // Cache read price
    cacheWrite?: number; // Cache write price
  };
  contextWindow?: number;
  maxTokens?: number; // Maximum output token count
  compat?: {
    maxTokensField?: string; // Request field name for the max output token count (e.g. "max_tokens")
  };
}

// OpenClaw default model config (agents.defaults.model)
export interface OpenClawDefaultModel {
  primary: string;
  fallbacks?: string[];
}

// OpenClaw model catalog entry (value inside agents.defaults.models)
export interface OpenClawModelCatalogEntry {
  alias?: string;
}

export interface OpenClawHealthWarning {
  code: string;
  message: string;
  path?: string;
}

export interface OpenClawWriteOutcome {
  backupPath?: string;
  warnings: OpenClawHealthWarning[];
}

export type OpenClawToolsProfile = "minimal" | "coding" | "messaging" | "full";

// OpenClaw provider config (settings_config structure)
// Corresponds to OpenClaw's models.providers.<provider-id> config
export interface OpenClawProviderConfig {
  baseUrl?: string; // API endpoint
  apiKey?: string; // API key
  api?: string; // API protocol type (e.g. "openai-completions", "anthropic")
  models?: OpenClawModel[]; // Available model list
  headers?: Record<string, string>; // Custom request headers (e.g. User-Agent)
  authHeader?: boolean; // Provider-specific auth toggle (e.g. Longcat)
}

// OpenClaw agents.defaults full config
export interface OpenClawAgentsDefaults {
  model?: OpenClawDefaultModel;
  models?: Record<string, OpenClawModelCatalogEntry>;
  timeoutSeconds?: number;
  timeout?: number;
  [key: string]: unknown; // preserve unknown fields
}

// OpenClaw env config (the env node of openclaw.json)
export interface OpenClawEnvConfig {
  [key: string]: unknown;
}

// OpenClaw tools config (the tools node of openclaw.json)
export interface OpenClawToolsConfig {
  profile?: OpenClawToolsProfile | string;
  allow?: string[];
  deny?: string[];
  [key: string]: unknown; // preserve unknown fields
}

// ============================================================================
// Hermes Agent-specific config
// ============================================================================

export interface HermesModelConfig {
  default?: string;
  provider?: string;
  base_url?: string;
  context_length?: number;
  max_tokens?: number;
  [key: string]: unknown;
}

export type HermesMemoryKind = "memory" | "user";

export interface HermesMemoryLimits {
  memory: number;
  user: number;
  memoryEnabled: boolean;
  userEnabled: boolean;
}
