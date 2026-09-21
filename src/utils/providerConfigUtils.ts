// Provider configuration processing utilities

import type { TemplateValueConfig } from "../config/claudeProviderPresets";
import type { CodexApiFormat } from "@/types";
import { deepClone } from "@/utils/deepClone";
import { normalizeTomlText } from "@/utils/textNormalization";
import { parse as parseToml } from "smol-toml";

const isPlainObject = (value: unknown): value is Record<string, unknown> => {
  return Object.prototype.toString.call(value) === "[object Object]";
};

/**
 * Keys that must be skipped while traversing config objects.
 *
 * `JSON.parse('{"__proto__":{…}}')` produces a `__proto__` that is an
 * **own enumerable property**, so `Object.entries` picks it up; and
 * `isPlainObject(target["__proto__"])` returns true for `Object.prototype`,
 * so the recursion writes straight into the global prototype. The common
 * config snippet (`settings.common_config_*`) can be overwritten by a
 * WebDAV/S3-synced remote, so this path is reachable without any XSS.
 *
 * In the normal flow, snippets have already been through `sanitizeSnippet`
 * at the entry point, so the three traversal functions below
 * (`deepMerge` / `deepRemove` / `isSubset`) never see these keys again; each
 * still carries its own check so that every function **stays safe when used
 * on its own**, without relying on callers remembering to sanitize first.
 */
const FORBIDDEN_MERGE_KEYS = new Set(["__proto__", "constructor", "prototype"]);

/**
 * Recursively strips forbidden keys, producing "the snippet that would
 * actually be written to the config".
 *
 * Only used by the **read side** (`hasCommonConfigSnippet` /
 * `hasTomlCommonConfigSnippet`) — the write side doesn't need it, since
 * `deepMerge` / `deepRemove` already skip forbidden keys themselves, so
 * their output is identical before and after sanitizing.
 *
 * Why the read side must do this: the two sides handle forbidden keys with
 * **different semantics**. The write side "skips this key and keeps
 * processing the rest", while `isSubset` must "reject outright on seeing a
 * forbidden key" for safety. So `{"env":{"A":"1"},"__proto__":{}}` would
 * actually write `env.A` yet be judged "not applied" — the snippet is
 * partially in effect while the toggle permanently shows disabled.
 *
 * Having the read side sanitize first means the comparison uses exactly
 * what the write side would produce, so the two sides never disagree.
 */
const sanitizeSnippet = (value: unknown): unknown => {
  if (Array.isArray(value)) return value.map(sanitizeSnippet);
  if (!isPlainObject(value)) return value;

  const cleaned: Record<string, unknown> = {};
  for (const [key, child] of Object.entries(value)) {
    if (FORBIDDEN_MERGE_KEYS.has(key)) continue;
    cleaned[key] = sanitizeSnippet(child);
  }
  return cleaned;
};

const deepMerge = (
  target: Record<string, unknown>,
  source: Record<string, unknown>,
): Record<string, unknown> => {
  Object.entries(source).forEach(([key, value]) => {
    if (FORBIDDEN_MERGE_KEYS.has(key)) return;

    if (isPlainObject(value)) {
      const existing = target[key];
      const nested = isPlainObject(existing) ? existing : {};
      target[key] = nested;
      deepMerge(nested, value);
    } else {
      // Overwrite non-object fields directly (arrays/primitives)
      target[key] = value;
    }
  });
  return target;
};

const deepRemove = (
  target: Record<string, unknown>,
  source: Record<string, unknown>,
) => {
  Object.entries(source).forEach(([key, value]) => {
    // Same as deepMerge, but more dangerous here: `"__proto__" in target` is
    // always true (`in` walks the prototype chain); without the skip, this
    // would recurse into `Object.prototype` and `delete` its properties.
    if (FORBIDDEN_MERGE_KEYS.has(key)) return;
    if (!(key in target)) return;

    if (isPlainObject(value) && isPlainObject(target[key])) {
      // Only remove nested properties that match exactly
      deepRemove(target[key], value);
      if (Object.keys(target[key]).length === 0) {
        delete target[key];
      }
    } else if (isSubset(target[key], value)) {
      // Only delete when the value matches exactly
      delete target[key];
    }
  });
};

const isSubset = (target: unknown, source: unknown): boolean => {
  if (isPlainObject(source)) {
    if (!isPlainObject(target)) return false;
    return Object.entries(source).every(([key, value]) => {
      // Fallback (the normal flow already strips this via sanitizeSnippet).
      // This function only reads, so it can't pollute the prototype, but
      // without this guard it would fall through to `target["__proto__"]`
      // (index lookup walks the prototype chain) and compare against
      // `Object.prototype` — `{"__proto__":{}}` would be judged a subset of
      // **any** config. Reject rather than skip: if some caller bypasses
      // sanitizing, a false "not applied" (the user clicks again; merging is
      // idempotent) is safer than a false "already applied".
      if (FORBIDDEN_MERGE_KEYS.has(key)) return false;
      // An inherited key doesn't count as "this config has the entry" — it must be an own property.
      if (!Object.prototype.hasOwnProperty.call(target, key)) return false;
      return isSubset(target[key], value);
    });
  }

  if (Array.isArray(source)) {
    if (!Array.isArray(target) || target.length !== source.length) return false;
    return source.every((item, index) => isSubset(target[index], item));
  }

  return target === source;
};

export interface UpdateCommonConfigResult {
  updatedConfig: string;
  error?: string;
}

// Validate JSON config format
export const validateJsonConfig = (
  value: string,
  fieldName: string = "config",
): string => {
  if (!value.trim()) {
    return "";
  }
  try {
    const parsed = JSON.parse(value);
    if (!parsed || typeof parsed !== "object" || Array.isArray(parsed)) {
      return `${fieldName} must be a JSON object`;
    }
    return "";
  } catch {
    return `${fieldName} has invalid JSON syntax, please check it`;
  }
};

// Write/remove the common config snippet in settingsConfig
export const updateCommonConfigSnippet = (
  jsonString: string,
  snippetString: string,
  enabled: boolean,
): UpdateCommonConfigResult => {
  let config: Record<string, unknown>;
  try {
    config = jsonString ? JSON.parse(jsonString) : {};
  } catch {
    return {
      updatedConfig: jsonString,
      error: "Failed to parse the config JSON; cannot apply the common config",
    };
  }

  if (!snippetString.trim()) {
    return {
      updatedConfig: JSON.stringify(config, null, 2),
    };
  }

  // Use the shared validation function
  const snippetError = validateJsonConfig(
    snippetString,
    "Common config snippet",
  );
  if (snippetError) {
    return {
      updatedConfig: JSON.stringify(config, null, 2),
      error: snippetError,
    };
  }

  // No need to sanitize here: deepMerge / deepRemove already skip forbidden
  // keys themselves, so output is byte-identical before and after sanitizing.
  // It's the **read side** (hasCommonConfigSnippet) that needs sanitizing —
  // see sanitizeSnippet for why.
  const snippet = JSON.parse(snippetString) as Record<string, unknown>;

  if (enabled) {
    const merged = deepMerge(deepClone(config), snippet);
    return {
      updatedConfig: JSON.stringify(merged, null, 2),
    };
  }

  const cloned = deepClone(config);
  deepRemove(cloned, snippet);
  return {
    updatedConfig: JSON.stringify(cloned, null, 2),
  };
};

// Check whether the current config already contains the common config snippet
export const hasCommonConfigSnippet = (
  jsonString: string,
  snippetString: string,
): boolean => {
  try {
    if (!snippetString.trim()) return false;
    const config = jsonString ? JSON.parse(jsonString) : {};
    const parsed = JSON.parse(snippetString);
    if (!isPlainObject(parsed)) return false;
    // Compare against the same sanitized result used by updateCommonConfigSnippet.
    const snippet = sanitizeSnippet(parsed);
    // If every key is forbidden, sanitizing yields an empty object — an
    // empty snippet applies nothing, so it must not report "enabled"
    // (`isSubset(config, {})` is true for any config).
    if (!isPlainObject(snippet) || Object.keys(snippet).length === 0) {
      return false;
    }
    return isSubset(config, snippet);
  } catch {
    return false;
  }
};

// Read the API key from the config (supports Claude, Codex, Gemini)
export const getApiKeyFromConfig = (
  jsonString: string,
  appType?: string,
): string => {
  try {
    const config = JSON.parse(jsonString);

    // Check the top-level apiKey field first (used by presets like Bedrock API Key)
    if (
      typeof config?.apiKey === "string" &&
      config.apiKey &&
      !config.apiKey.includes("${")
    ) {
      return config.apiKey;
    }

    const env = config?.env;

    if (!env) return "";

    // Gemini API Key
    if (appType === "gemini") {
      const geminiKey = env.GEMINI_API_KEY;
      return typeof geminiKey === "string" ? geminiKey : "";
    }

    // Codex API Key
    if (appType === "codex") {
      const codexKey = env.CODEX_API_KEY;
      return typeof codexKey === "string" ? codexKey : "";
    }

    // Claude API Key (prefers ANTHROPIC_AUTH_TOKEN, then ANTHROPIC_API_KEY)
    const token = env.ANTHROPIC_AUTH_TOKEN;
    const apiKey = env.ANTHROPIC_API_KEY;
    const value =
      typeof token === "string"
        ? token
        : typeof apiKey === "string"
          ? apiKey
          : "";
    return value;
  } catch {
    return "";
  }
};

// Template variable substitution
export const applyTemplateValues = (
  config: unknown,
  templateValues: Record<string, TemplateValueConfig> | undefined,
): unknown => {
  const resolvedValues = Object.fromEntries(
    Object.entries(templateValues ?? {}).map(([key, value]) => {
      const resolvedValue =
        value.editorValue !== undefined
          ? value.editorValue
          : (value.defaultValue ?? "");
      return [key, resolvedValue];
    }),
  );

  const replaceInString = (str: string): string => {
    return Object.entries(resolvedValues).reduce((acc, [key, value]) => {
      const placeholder = `\${${key}}`;
      if (!acc.includes(placeholder)) {
        return acc;
      }
      return acc.split(placeholder).join(value ?? "");
    }, str);
  };

  const traverse = (obj: unknown): unknown => {
    if (typeof obj === "string") {
      return replaceInString(obj);
    }
    if (Array.isArray(obj)) {
      return obj.map(traverse);
    }
    if (obj && typeof obj === "object") {
      const result: Record<string, unknown> = {};
      for (const [key, value] of Object.entries(obj)) {
        result[key] = traverse(value);
      }
      return result;
    }
    return obj;
  };

  return traverse(config);
};

// Check whether an API key field exists in the config
export const hasApiKeyField = (
  jsonString: string,
  appType?: string,
): boolean => {
  try {
    const config = JSON.parse(jsonString);

    // Check the top-level apiKey field (used by presets like Bedrock API Key)
    if (Object.prototype.hasOwnProperty.call(config, "apiKey")) {
      return true;
    }

    const env = config?.env ?? {};

    if (appType === "gemini") {
      return Object.prototype.hasOwnProperty.call(env, "GEMINI_API_KEY");
    }

    if (appType === "codex") {
      return Object.prototype.hasOwnProperty.call(env, "CODEX_API_KEY");
    }

    return (
      Object.prototype.hasOwnProperty.call(env, "ANTHROPIC_AUTH_TOKEN") ||
      Object.prototype.hasOwnProperty.call(env, "ANTHROPIC_API_KEY")
    );
  } catch {
    return false;
  }
};

// Write/update the API key in the config; does not add missing fields by default
export const setApiKeyInConfig = (
  jsonString: string,
  apiKey: string,
  options: {
    createIfMissing?: boolean;
    appType?: string;
    apiKeyField?: string;
  } = {},
): string => {
  const { createIfMissing = false, appType, apiKeyField } = options;
  try {
    const config = JSON.parse(jsonString);

    // Check the top-level apiKey field first (used by presets like Bedrock API Key)
    if (Object.prototype.hasOwnProperty.call(config, "apiKey")) {
      config.apiKey = apiKey;
      return JSON.stringify(config, null, 2);
    }

    if (!config.env) {
      if (!createIfMissing) return jsonString;
      config.env = {};
    }
    const env = config.env as Record<string, unknown>;

    // Gemini API Key
    if (appType === "gemini") {
      if ("GEMINI_API_KEY" in env) {
        env.GEMINI_API_KEY = apiKey;
      } else if (createIfMissing) {
        env.GEMINI_API_KEY = apiKey;
      } else {
        return jsonString;
      }
      return JSON.stringify(config, null, 2);
    }

    // Codex API Key
    if (appType === "codex") {
      if ("CODEX_API_KEY" in env) {
        env.CODEX_API_KEY = apiKey;
      } else if (createIfMissing) {
        env.CODEX_API_KEY = apiKey;
      } else {
        return jsonString;
      }
      return JSON.stringify(config, null, 2);
    }

    // Claude API Key (prefer writing to whichever field already exists; if neither
    // exists and creation is allowed, use apiKeyField or default to AUTH_TOKEN)
    if ("ANTHROPIC_AUTH_TOKEN" in env) {
      env.ANTHROPIC_AUTH_TOKEN = apiKey;
    } else if ("ANTHROPIC_API_KEY" in env) {
      env.ANTHROPIC_API_KEY = apiKey;
    } else if (createIfMissing) {
      env[apiKeyField ?? "ANTHROPIC_AUTH_TOKEN"] = apiKey;
    } else {
      return jsonString;
    }
    return JSON.stringify(config, null, 2);
  } catch {
    return jsonString;
  }
};

// ========== TOML Config Utilities ==========

// Merging/stripping TOML snippets must go through the backend command
// (configApi.updateTomlCommonConfigSnippet, which uses toml_edit to preserve
// comments and key order). Never re-serialize the whole document on the
// frontend via smol-toml parse -> merge -> stringify: it drops all comments,
// reorders keys, and generates extra empty parent table headers.

// Check if TOML config already contains the common config snippet (structural subset check)
export const hasTomlCommonConfigSnippet = (
  tomlString: string,
  snippetString: string,
): boolean => {
  if (!snippetString.trim()) return false;

  try {
    const config = parseToml(normalizeTomlText(tomlString || ""));
    // Sanitize the same way as the JSON side: smol-toml also parses table headers like `["__proto__"]` into own keys.
    const snippet = sanitizeSnippet(
      parseToml(normalizeTomlText(snippetString)),
    );
    if (!isPlainObject(snippet) || Object.keys(snippet).length === 0) {
      return false;
    }
    return isSubset(config, snippet);
  } catch {
    // Fallback to text-based matching if TOML parsing fails
    const norm = (s: string) => s.replace(/\s+/g, " ").trim();
    return norm(tomlString).includes(norm(snippetString));
  }
};

// ========== Codex base_url utils ==========

const TOML_SECTION_HEADER_PATTERN = /^\s*\[([^\]\r\n]+)\]\s*$/;
const TOML_BASE_URL_PATTERN =
  /^\s*base_url\s*=\s*(?:"((?:\\.|[^"\\\r\n])*)"|'([^'\r\n]*)')\s*(?:#.*)?$/;
const TOML_EXPERIMENTAL_BEARER_TOKEN_PATTERN =
  /^\s*experimental_bearer_token\s*=\s*(["'])([^"'\r\n]+)\1\s*(?:#.*)?$/;
const TOML_EXPERIMENTAL_BEARER_TOKEN_REPLACE_PATTERN =
  /^(\s*experimental_bearer_token\s*=\s*)(?:"(?:\\.|[^"\\\r\n])*"|'[^'\r\n]*')(\s*(?:#.*)?)$/;
// Double-quoted basic string (supports escape sequences like \" \\, and
// recognizes setCodexModelName's own escaped output); needs unescaping after
// extraction. Deliberately does a strict whole-line match only, not a loose
// key-name match — a loose match would misfire on text inside multi-line
// strings that looks like an assignment; unrecognized odd values are on the
// user.
const TOML_MODEL_DOUBLE_QUOTED_PATTERN =
  /^\s*model\s*=\s*"((?:[^"\\\r\n]|\\.)*)"\s*(?:#.*)?$/;
// Single-quoted literal string (TOML semantics: no escaping)
const TOML_MODEL_SINGLE_QUOTED_PATTERN =
  /^\s*model\s*=\s*'([^'\r\n]*)'\s*(?:#.*)?$/;
const TOML_WIRE_API_PATTERN =
  /^\s*wire_api\s*=\s*(["'])([^"'\r\n]+)\1\s*(?:#.*)?$/;
const TOML_MODEL_PROVIDER_LINE_PATTERN =
  /^\s*model_provider\s*=\s*(["'])([^"'\r\n]+)\1\s*(?:#.*)?$/;
const TOML_PROVIDER_NAME_PATTERN =
  /^\s*name\s*=\s*(["'])([^"'\r\n]+)\1\s*(?:#.*)?$/;
const TOML_PROVIDER_NAME_REPLACE_PATTERN =
  /^(\s*name\s*=\s*)(?:"(?:\\.|[^"\\\r\n])*"|'[^'\r\n]*')(\s*(?:#.*)?)$/;
const CODEX_RESERVED_MODEL_PROVIDER_IDS = new Set([
  "amazon-bedrock",
  "openai",
  "ollama",
  "lmstudio",
  "oss",
  "ollama-chat",
]);

interface TomlSectionRange {
  bodyEndIndex: number;
  bodyStartIndex: number;
  headerLineIndex: number;
}

interface TomlAssignmentMatch {
  index: number;
  sectionName?: string;
  value: string;
}

const finalizeTomlText = (lines: string[]): string =>
  lines
    .join("\n")
    .replace(/\n{3,}/g, "\n\n")
    .replace(/^\n+/, "");

const getTomlSectionRange = (
  lines: string[],
  sectionName: string,
): TomlSectionRange | undefined => {
  let headerLineIndex = -1;

  for (let index = 0; index < lines.length; index += 1) {
    const match = lines[index].match(TOML_SECTION_HEADER_PATTERN);
    if (!match) {
      continue;
    }

    if (headerLineIndex === -1) {
      if (match[1] === sectionName) {
        headerLineIndex = index;
      }
      continue;
    }

    return {
      bodyStartIndex: headerLineIndex + 1,
      bodyEndIndex: index,
      headerLineIndex,
    };
  }

  if (headerLineIndex === -1) {
    return undefined;
  }

  return {
    bodyStartIndex: headerLineIndex + 1,
    bodyEndIndex: lines.length,
    headerLineIndex,
  };
};

const getTopLevelEndIndex = (lines: string[]): number => {
  const firstSectionIndex = lines.findIndex((line) =>
    TOML_SECTION_HEADER_PATTERN.test(line),
  );
  return firstSectionIndex === -1 ? lines.length : firstSectionIndex;
};

const getTomlSectionInsertIndex = (
  lines: string[],
  sectionRange: TomlSectionRange,
): number => {
  let insertIndex = sectionRange.bodyEndIndex;
  while (
    insertIndex > sectionRange.bodyStartIndex &&
    lines[insertIndex - 1].trim() === ""
  ) {
    insertIndex -= 1;
  }
  return insertIndex;
};

const getCodexModelProviderName = (configText: string): string | undefined => {
  const normalized = normalizeTomlText(configText);
  try {
    const parsed = parseToml(normalized) as Record<string, unknown>;
    const providerName =
      typeof parsed.model_provider === "string"
        ? parsed.model_provider.trim()
        : undefined;
    if (providerName) return providerName;
  } catch {
    // Fall back to a top-level line scan while the user is editing invalid TOML.
  }

  const lines = normalized.split("\n");
  const index = getTopLevelModelProviderLineIndex(lines);
  if (index === -1) return undefined;
  const match = lines[index].match(TOML_MODEL_PROVIDER_LINE_PATTERN);
  const providerName = match?.[2]?.trim();
  return providerName || undefined;
};

const isCodexUnifiedSessionProjection = (configText: string): boolean => {
  try {
    const parsed = parseToml(normalizeTomlText(configText)) as Record<
      string,
      unknown
    >;
    const modelProviders = parsed.model_providers;
    const custom = isPlainObject(modelProviders)
      ? modelProviders.custom
      : undefined;
    return (
      parsed.model_provider === "custom" &&
      isPlainObject(custom) &&
      Object.keys(custom).length === 4 &&
      custom.name === "OpenAI" &&
      custom.requires_openai_auth === true &&
      custom.supports_websockets === true &&
      custom.wire_api === "responses"
    );
  } catch {
    return false;
  }
};

export const hasExplicitNonOpenAiCodexModelProvider = (
  configText: string | undefined | null,
): boolean => {
  if (typeof configText !== "string") return false;
  if (isCodexUnifiedSessionProjection(configText)) return false;
  const providerName = getCodexModelProviderName(configText);
  return Boolean(
    providerName && providerName.trim().toLowerCase() !== "openai",
  );
};

const getCodexProviderSectionName = (
  configText: string,
): string | undefined => {
  const providerName = getCodexModelProviderName(configText);
  return providerName ? `model_providers.${providerName}` : undefined;
};

const isCustomCodexModelProviderId = (providerName: string): boolean => {
  const id = providerName.trim().toLowerCase();
  return Boolean(id) && !CODEX_RESERVED_MODEL_PROVIDER_IDS.has(id);
};

const getCodexCustomProviderSectionName = (
  configText: string,
): string | undefined => {
  const providerName = getCodexModelProviderName(configText);
  return providerName && isCustomCodexModelProviderId(providerName)
    ? `model_providers.${providerName}`
    : undefined;
};

const findTomlAssignmentInRange = (
  lines: string[],
  pattern: RegExp,
  startIndex: number,
  endIndex: number,
  sectionName?: string,
): TomlAssignmentMatch | undefined => {
  for (let index = startIndex; index < endIndex; index += 1) {
    const match = lines[index].match(pattern);
    const value = match?.[2] ?? match?.[1];
    if (value) {
      return {
        index,
        sectionName,
        value,
      };
    }
  }

  return undefined;
};

const findTomlAssignmentsInRange = (
  lines: string[],
  pattern: RegExp,
  startIndex: number,
  endIndex: number,
  sectionName?: string,
): TomlAssignmentMatch[] => {
  const matches: TomlAssignmentMatch[] = [];

  for (let index = startIndex; index < endIndex; index += 1) {
    const match = lines[index].match(pattern);
    const value = match?.[2] ?? match?.[1];
    if (value) {
      matches.push({ index, sectionName, value });
    }
  }

  return matches;
};

const findTomlLineInRange = (
  lines: string[],
  pattern: RegExp,
  startIndex: number,
  endIndex: number,
): number => {
  for (let index = startIndex; index < endIndex; index += 1) {
    if (pattern.test(lines[index])) {
      return index;
    }
  }

  return -1;
};

const findTomlAssignments = (
  lines: string[],
  pattern: RegExp,
): TomlAssignmentMatch[] => {
  const assignments: TomlAssignmentMatch[] = [];
  let currentSectionName: string | undefined;

  lines.forEach((line, index) => {
    const sectionMatch = line.match(TOML_SECTION_HEADER_PATTERN);
    if (sectionMatch) {
      currentSectionName = sectionMatch[1];
      return;
    }

    const match = line.match(pattern);
    const value = match?.[2] ?? match?.[1];
    if (!value) {
      return;
    }

    assignments.push({
      index,
      sectionName: currentSectionName,
      value,
    });
  });

  return assignments;
};

const isMcpServerSection = (sectionName?: string): boolean =>
  sectionName === "mcp_servers" ||
  sectionName?.startsWith("mcp_servers.") === true;

const isOtherProviderSection = (
  sectionName: string | undefined,
  targetSectionName: string | undefined,
): boolean =>
  Boolean(
    sectionName &&
      sectionName !== targetSectionName &&
      (sectionName === "model_providers" ||
        sectionName.startsWith("model_providers.")),
  );

const getRecoverableBaseUrlAssignments = (
  assignments: TomlAssignmentMatch[],
  targetSectionName: string | undefined,
): TomlAssignmentMatch[] =>
  assignments.filter(
    ({ sectionName }) =>
      sectionName !== targetSectionName &&
      !isMcpServerSection(sectionName) &&
      !isOtherProviderSection(sectionName, targetSectionName),
  );

const getRecoverableCodexProviderAssignments = getRecoverableBaseUrlAssignments;

const getTopLevelModelProviderLineIndex = (lines: string[]): number => {
  const topLevelEndIndex = getTopLevelEndIndex(lines);

  for (let index = 0; index < topLevelEndIndex; index += 1) {
    if (TOML_MODEL_PROVIDER_LINE_PATTERN.test(lines[index])) {
      return index;
    }
  }

  return -1;
};

const TOML_BASIC_STRING_ESCAPES: Record<string, string> = {
  '"': '\\"',
  "\\": "\\\\",
  "\b": "\\b",
  "\t": "\\t",
  "\n": "\\n",
  "\f": "\\f",
  "\r": "\\r",
};

const escapeTomlBasicString = (value: string): string =>
  // eslint-disable-next-line no-control-regex -- TOML requires escaping these control characters
  value.replace(/["\\\u0000-\u001f]/g, (ch) => {
    const escaped = TOML_BASIC_STRING_ESCAPES[ch];
    if (escaped) return escaped;
    return `\\u${ch.charCodeAt(0).toString(16).padStart(4, "0")}`;
  });

const tomlBasicString = (value: string): string =>
  `"${escapeTomlBasicString(value)}"`;

const TOML_BASIC_STRING_UNESCAPES: Record<string, string> = {
  '"': '"',
  "\\": "\\",
  b: "\b",
  t: "\t",
  n: "\n",
  f: "\f",
  r: "\r",
};

// Inverse of escapeTomlBasicString; unknown escape sequences are kept as-is
const unescapeTomlBasicString = (value: string): string =>
  value.replace(
    /\\(?:u([0-9a-fA-F]{4})|U([0-9a-fA-F]{8})|(.))/g,
    (match, u4, u8, ch) => {
      if (u4 || u8) return String.fromCodePoint(parseInt(u4 || u8, 16));
      return TOML_BASIC_STRING_UNESCAPES[ch] ?? match;
    },
  );

const CODEX_CHAT_WIRE_API_VALUES = new Set([
  "chat",
  "chat_completions",
  "chat-completions",
  "openai_chat",
  "openai-chat",
  "openai_chat_completions",
]);

// Check whether the given wire_api string represents Codex's Chat Completions protocol
export const isCodexChatWireApi = (
  wireApi: string | undefined | null,
): boolean =>
  CODEX_CHAT_WIRE_API_VALUES.has((wireApi ?? "").trim().toLowerCase());

export const isCodexAnthropicWireApi = (
  wireApi: string | undefined | null,
): boolean =>
  [
    "anthropic",
    "anthropic_messages",
    "anthropic-messages",
    "messages",
    "claude",
  ].includes((wireApi ?? "").trim().toLowerCase());

export const codexApiFormatFromWireApi = (
  wireApi: string | undefined | null,
): CodexApiFormat | undefined => {
  if (isCodexChatWireApi(wireApi)) return "openai_chat";
  if (isCodexAnthropicWireApi(wireApi)) return "anthropic";
  switch ((wireApi ?? "").trim().toLowerCase()) {
    case "responses":
    case "openai_responses":
    case "openai-responses":
      return "openai_responses";
    default:
      return undefined;
  }
};

// Extract wire_api from Codex's TOML config text (supports single/double quotes)
export const extractCodexWireApi = (
  configText: string | undefined | null,
): string | undefined => {
  try {
    const raw = typeof configText === "string" ? configText : "";
    const text = normalizeTomlText(raw);
    if (!text) return undefined;

    const lines = text.split("\n");
    const targetSectionName = getCodexProviderSectionName(text);

    if (targetSectionName) {
      const sectionRange = getTomlSectionRange(lines, targetSectionName);
      if (sectionRange) {
        const match = findTomlAssignmentInRange(
          lines,
          TOML_WIRE_API_PATTERN,
          sectionRange.bodyStartIndex,
          sectionRange.bodyEndIndex,
          targetSectionName,
        );
        if (match?.value) {
          return match.value;
        }
      }
    }

    const topLevelMatch = findTomlAssignmentInRange(
      lines,
      TOML_WIRE_API_PATTERN,
      0,
      getTopLevelEndIndex(lines),
    );
    if (topLevelMatch?.value) {
      return topLevelMatch.value;
    }

    const fallbackAssignments = getRecoverableCodexProviderAssignments(
      findTomlAssignments(lines, TOML_WIRE_API_PATTERN),
      targetSectionName,
    );
    return fallbackAssignments.length === 1
      ? fallbackAssignments[0].value
      : undefined;
  } catch {
    return undefined;
  }
};

// Write or update the wire_api field in Codex's TOML config text
export const setCodexWireApi = (
  configText: string,
  wireApi: "responses" | "chat",
): string => {
  const normalizedText = normalizeTomlText(configText);
  const lines = normalizedText ? normalizedText.split("\n") : [];
  const targetSectionName = getCodexProviderSectionName(normalizedText);
  const replacementLine = `wire_api = "${wireApi}"`;
  const allAssignments = findTomlAssignments(lines, TOML_WIRE_API_PATTERN);
  const recoverableAssignments = getRecoverableCodexProviderAssignments(
    allAssignments,
    targetSectionName,
  );

  if (targetSectionName) {
    let targetSectionRange = getTomlSectionRange(lines, targetSectionName);
    const targetMatch = targetSectionRange
      ? findTomlAssignmentInRange(
          lines,
          TOML_WIRE_API_PATTERN,
          targetSectionRange.bodyStartIndex,
          targetSectionRange.bodyEndIndex,
          targetSectionName,
        )
      : undefined;

    if (targetMatch) {
      lines[targetMatch.index] = replacementLine;
      return finalizeTomlText(lines);
    }

    if (recoverableAssignments.length === 1) {
      lines.splice(recoverableAssignments[0].index, 1);
      targetSectionRange = getTomlSectionRange(lines, targetSectionName);
    }

    if (targetSectionRange) {
      const insertIndex = getTomlSectionInsertIndex(lines, targetSectionRange);
      lines.splice(insertIndex, 0, replacementLine);
      return finalizeTomlText(lines);
    }

    if (lines.length > 0 && lines[lines.length - 1].trim() !== "") {
      lines.push("");
    }
    lines.push(`[${targetSectionName}]`, replacementLine);
    return finalizeTomlText(lines);
  }

  const topLevelEndIndex = getTopLevelEndIndex(lines);
  const topLevelMatch = findTomlAssignmentInRange(
    lines,
    TOML_WIRE_API_PATTERN,
    0,
    topLevelEndIndex,
  );
  if (topLevelMatch) {
    lines[topLevelMatch.index] = replacementLine;
    return finalizeTomlText(lines);
  }

  const modelProviderIndex = getTopLevelModelProviderLineIndex(lines);
  if (modelProviderIndex !== -1) {
    lines.splice(modelProviderIndex + 1, 0, replacementLine);
    return finalizeTomlText(lines);
  }

  if (lines.length === 0) {
    return `${replacementLine}\n`;
  }

  lines.splice(topLevelEndIndex, 0, replacementLine);
  return finalizeTomlText(lines);
};

export const isCodexRemoteCompactionEnabled = (
  configText: string | undefined | null,
): boolean => {
  try {
    const raw = typeof configText === "string" ? configText : "";
    const text = normalizeTomlText(raw);
    if (!text) return false;

    try {
      const parsed = parseToml(text) as Record<string, unknown>;
      const providerId =
        typeof parsed.model_provider === "string"
          ? parsed.model_provider.trim()
          : "";
      if (!providerId || !isCustomCodexModelProviderId(providerId)) {
        return false;
      }

      const modelProviders = parsed.model_providers;
      if (!isPlainObject(modelProviders)) return false;
      const modelProvider = modelProviders[providerId];
      return isPlainObject(modelProvider) && modelProvider.name === "OpenAI";
    } catch {
      // Fall back to line scanning while the user is editing invalid TOML.
    }

    const lines = text.split("\n");
    const targetSectionName = getCodexCustomProviderSectionName(text);
    if (!targetSectionName) return false;

    const sectionRange = getTomlSectionRange(lines, targetSectionName);
    if (!sectionRange) return false;

    const nameAssignment = findTomlAssignmentInRange(
      lines,
      TOML_PROVIDER_NAME_PATTERN,
      sectionRange.bodyStartIndex,
      sectionRange.bodyEndIndex,
      targetSectionName,
    );

    return nameAssignment?.value === "OpenAI";
  } catch {
    return false;
  }
};

export const setCodexRemoteCompaction = (
  configText: string,
  enabled: boolean,
  fallbackProviderName?: string,
): string => {
  const normalizedText = normalizeTomlText(configText);
  const lines = normalizedText ? normalizedText.split("\n") : [];
  const targetSectionName = getCodexCustomProviderSectionName(normalizedText);

  if (!targetSectionName) {
    return normalizedText;
  }

  let targetSectionRange = getTomlSectionRange(lines, targetSectionName);
  const replacementName = enabled
    ? "OpenAI"
    : fallbackProviderName?.trim() ||
      getCodexModelProviderName(normalizedText) ||
      "custom";
  const replacementLine = `name = ${tomlBasicString(replacementName)}`;

  if (targetSectionRange) {
    const nameLine = findTomlLineInRange(
      lines,
      TOML_PROVIDER_NAME_REPLACE_PATTERN,
      targetSectionRange.bodyStartIndex,
      targetSectionRange.bodyEndIndex,
    );

    if (nameLine !== -1) {
      lines[nameLine] = lines[nameLine].replace(
        TOML_PROVIDER_NAME_REPLACE_PATTERN,
        `$1${tomlBasicString(replacementName)}$2`,
      );
      return finalizeTomlText(lines);
    }

    lines.splice(
      getTomlSectionInsertIndex(lines, targetSectionRange),
      0,
      replacementLine,
    );
    return finalizeTomlText(lines);
  }

  if (!enabled) return normalizedText;

  if (lines.length > 0 && lines[lines.length - 1].trim() !== "") {
    lines.push("");
  }

  lines.push(`[${targetSectionName}]`, replacementLine);
  targetSectionRange = getTomlSectionRange(lines, targetSectionName);
  return targetSectionRange ? finalizeTomlText(lines) : normalizedText;
};

// Extract base_url from Codex's TOML config text (supports single/double quotes)
export const extractCodexBaseUrl = (
  configText: string | undefined | null,
): string | undefined => {
  try {
    const raw = typeof configText === "string" ? configText : "";
    const text = normalizeTomlText(raw);
    if (!text) return undefined;

    const lines = text.split("\n");
    const targetSectionName = getCodexProviderSectionName(text);

    if (targetSectionName) {
      const sectionRange = getTomlSectionRange(lines, targetSectionName);
      if (sectionRange) {
        const match = findTomlAssignmentInRange(
          lines,
          TOML_BASE_URL_PATTERN,
          sectionRange.bodyStartIndex,
          sectionRange.bodyEndIndex,
          targetSectionName,
        );
        if (match?.value) {
          return match.value;
        }
      }
    }

    const topLevelMatch = findTomlAssignmentInRange(
      lines,
      TOML_BASE_URL_PATTERN,
      0,
      getTopLevelEndIndex(lines),
    );
    if (topLevelMatch?.value) {
      return topLevelMatch.value;
    }

    const fallbackAssignments = getRecoverableBaseUrlAssignments(
      findTomlAssignments(lines, TOML_BASE_URL_PATTERN),
      targetSectionName,
    );
    return fallbackAssignments.length === 1
      ? fallbackAssignments[0].value
      : undefined;
  } catch {
    return undefined;
  }
};

// Extract experimental_bearer_token from Codex's TOML config text (Mobile-mode compatibility)
export const extractCodexExperimentalBearerToken = (
  configText: string | undefined | null,
): string | undefined => {
  try {
    const raw = typeof configText === "string" ? configText : "";
    const text = normalizeTomlText(raw);
    if (!text) return undefined;

    try {
      const parsed = parseToml(text) as Record<string, unknown>;
      const providerName =
        typeof parsed.model_provider === "string"
          ? parsed.model_provider.trim()
          : undefined;
      const modelProviders = parsed.model_providers;
      const modelProvider =
        providerName &&
        isCustomCodexModelProviderId(providerName) &&
        isPlainObject(modelProviders)
          ? modelProviders[providerName]
          : undefined;
      const providerToken =
        isPlainObject(modelProvider) &&
        typeof modelProvider.experimental_bearer_token === "string"
          ? modelProvider.experimental_bearer_token.trim()
          : undefined;
      if (providerToken) return providerToken;
      const topLevelToken =
        typeof parsed.experimental_bearer_token === "string"
          ? parsed.experimental_bearer_token.trim()
          : undefined;
      if (topLevelToken) return topLevelToken;
    } catch {
      // Fall back to the line scanner for partially edited TOML.
    }

    const lines = text.split("\n");
    const targetSectionName = getCodexCustomProviderSectionName(text);

    if (targetSectionName) {
      const sectionRange = getTomlSectionRange(lines, targetSectionName);
      if (sectionRange) {
        const match = findTomlAssignmentInRange(
          lines,
          TOML_EXPERIMENTAL_BEARER_TOKEN_PATTERN,
          sectionRange.bodyStartIndex,
          sectionRange.bodyEndIndex,
          targetSectionName,
        );
        if (match?.value) {
          return match.value;
        }
      }
    }

    const topLevelMatch = findTomlAssignmentInRange(
      lines,
      TOML_EXPERIMENTAL_BEARER_TOKEN_PATTERN,
      0,
      getTopLevelEndIndex(lines),
    );
    return topLevelMatch?.value;
  } catch {
    return undefined;
  }
};

// Synchronously update the existing experimental_bearer_token in Codex's config.toml.
// Only modifies an entry that already exists; never adds one — avoids
// breaking a plain third-party config that doesn't use Mobile compat mode.
// Deletes the line when the token is empty (so the user can actually clear
// the API key, instead of pickCodexApiKey's fallback filling it back in).
export const updateCodexExperimentalBearerToken = (
  configText: string,
  token: string,
): string => {
  const normalizedText = normalizeTomlText(configText);
  if (
    !normalizedText ||
    !normalizedText.includes("experimental_bearer_token")
  ) {
    return configText;
  }

  const lines = normalizedText.split("\n");
  const targetSectionName = getCodexCustomProviderSectionName(normalizedText);

  let tokenLineIndex = -1;
  if (targetSectionName) {
    const sectionRange = getTomlSectionRange(lines, targetSectionName);
    if (sectionRange) {
      const index = findTomlLineInRange(
        lines,
        TOML_EXPERIMENTAL_BEARER_TOKEN_REPLACE_PATTERN,
        sectionRange.bodyStartIndex,
        sectionRange.bodyEndIndex,
      );
      if (index !== -1) tokenLineIndex = index;
    }
  }
  if (tokenLineIndex === -1) {
    const topLevelIndex = findTomlLineInRange(
      lines,
      TOML_EXPERIMENTAL_BEARER_TOKEN_REPLACE_PATTERN,
      0,
      getTopLevelEndIndex(lines),
    );
    if (topLevelIndex !== -1) tokenLineIndex = topLevelIndex;
  }

  if (tokenLineIndex === -1) return configText;

  const trimmed = token.trim();
  if (!trimmed) {
    lines.splice(tokenLineIndex, 1);
  } else {
    const escaped = escapeTomlBasicString(trimmed);
    const existingLine = lines[tokenLineIndex];
    lines[tokenLineIndex] = TOML_EXPERIMENTAL_BEARER_TOKEN_REPLACE_PATTERN.test(
      existingLine,
    )
      ? existingLine.replace(
          TOML_EXPERIMENTAL_BEARER_TOKEN_REPLACE_PATTERN,
          `$1"${escaped}"$2`,
        )
      : `experimental_bearer_token = "${escaped}"`;
  }
  return finalizeTomlText(lines);
};

// Extract the Codex base_url from a Provider object (when settingsConfig.config is a TOML string)
export const getCodexBaseUrl = (
  provider: { settingsConfig?: Record<string, unknown> } | undefined | null,
): string | undefined => {
  try {
    const config = provider?.settingsConfig?.config;
    const text = typeof config === "string" ? config : "";
    return extractCodexBaseUrl(text);
  } catch {
    return undefined;
  }
};

// Write or update the base_url field in Codex's TOML config text
export const setCodexBaseUrl = (
  configText: string,
  baseUrl: string,
): string => {
  const trimmed = baseUrl.trim();
  const normalizedText = normalizeTomlText(configText);
  const lines = normalizedText ? normalizedText.split("\n") : [];
  const targetSectionName = getCodexProviderSectionName(normalizedText);
  const allAssignments = findTomlAssignments(lines, TOML_BASE_URL_PATTERN);
  const recoverableAssignments = getRecoverableBaseUrlAssignments(
    allAssignments,
    targetSectionName,
  );

  if (!trimmed) {
    if (!normalizedText) return normalizedText;

    if (targetSectionName) {
      const sectionRange = getTomlSectionRange(lines, targetSectionName);
      const targetMatches = sectionRange
        ? findTomlAssignmentsInRange(
            lines,
            TOML_BASE_URL_PATTERN,
            sectionRange.bodyStartIndex,
            sectionRange.bodyEndIndex,
            targetSectionName,
          )
        : [];

      if (targetMatches.length > 0) {
        for (const match of [...targetMatches].reverse()) {
          lines.splice(match.index, 1);
        }
        return finalizeTomlText(lines);
      }
    }

    if (recoverableAssignments.length === 1) {
      lines.splice(recoverableAssignments[0].index, 1);
      return finalizeTomlText(lines);
    }

    return finalizeTomlText(lines);
  }

  const normalizedUrl = trimmed.replace(/\s+/g, "");
  const replacementLine = `base_url = ${tomlBasicString(normalizedUrl)}`;

  if (targetSectionName) {
    let targetSectionRange = getTomlSectionRange(lines, targetSectionName);
    const targetMatches = targetSectionRange
      ? findTomlAssignmentsInRange(
          lines,
          TOML_BASE_URL_PATTERN,
          targetSectionRange.bodyStartIndex,
          targetSectionRange.bodyEndIndex,
          targetSectionName,
        )
      : [];

    if (targetMatches.length > 0) {
      const [firstMatch, ...duplicateMatches] = targetMatches;
      lines[firstMatch.index] = replacementLine;
      for (const match of [...duplicateMatches].reverse()) {
        lines.splice(match.index, 1);
      }
      return finalizeTomlText(lines);
    }

    if (recoverableAssignments.length === 1) {
      lines.splice(recoverableAssignments[0].index, 1);
      targetSectionRange = getTomlSectionRange(lines, targetSectionName);
    }

    if (targetSectionRange) {
      const insertIndex = getTomlSectionInsertIndex(lines, targetSectionRange);
      lines.splice(insertIndex, 0, replacementLine);
      return finalizeTomlText(lines);
    }

    if (lines.length > 0 && lines[lines.length - 1].trim() !== "") {
      lines.push("");
    }
    lines.push(`[${targetSectionName}]`, replacementLine);
    return finalizeTomlText(lines);
  }

  const topLevelEndIndex = getTopLevelEndIndex(lines);
  const topLevelMatches = findTomlAssignmentsInRange(
    lines,
    TOML_BASE_URL_PATTERN,
    0,
    topLevelEndIndex,
  );
  if (topLevelMatches.length > 0) {
    const [firstMatch, ...duplicateMatches] = topLevelMatches;
    lines[firstMatch.index] = replacementLine;
    for (const match of [...duplicateMatches].reverse()) {
      lines.splice(match.index, 1);
    }
    return finalizeTomlText(lines);
  }

  const modelProviderIndex = getTopLevelModelProviderLineIndex(lines);
  if (modelProviderIndex !== -1) {
    lines.splice(modelProviderIndex + 1, 0, replacementLine);
    return finalizeTomlText(lines);
  }

  if (lines.length === 0) {
    return `${replacementLine}\n`;
  }

  const insertIndex = topLevelEndIndex;
  lines.splice(insertIndex, 0, replacementLine);
  return finalizeTomlText(lines);
};

// ========== Codex model name utils ==========

// The first model line in the top-level range recognized by strict mode; -1 means none
const findTopLevelModelLineIndex = (
  lines: string[],
  topLevelEndIndex: number,
): number => {
  for (let i = 0; i < topLevelEndIndex; i += 1) {
    if (
      TOML_MODEL_DOUBLE_QUOTED_PATTERN.test(lines[i]) ||
      TOML_MODEL_SINGLE_QUOTED_PATTERN.test(lines[i])
    ) {
      return i;
    }
  }
  return -1;
};

// Extract the model field from Codex's TOML config text (supports single/
// double quotes; double-quoted strings are unescaped as TOML basic strings
// to guarantee a round trip with setCodexModelName)
export const extractCodexModelName = (
  configText: string | undefined | null,
): string | undefined => {
  try {
    const raw = typeof configText === "string" ? configText : "";
    const text = normalizeTomlText(raw);
    if (!text) return undefined;
    const lines = text.split("\n");
    const topLevelEndIndex = getTopLevelEndIndex(lines);
    for (let i = 0; i < topLevelEndIndex; i += 1) {
      const doubleQuoted = lines[i].match(TOML_MODEL_DOUBLE_QUOTED_PATTERN);
      if (doubleQuoted) return unescapeTomlBasicString(doubleQuoted[1]);
      const singleQuoted = lines[i].match(TOML_MODEL_SINGLE_QUOTED_PATTERN);
      if (singleQuoted) return singleQuoted[1];
    }
    return undefined;
  } catch {
    return undefined;
  }
};

// Write or update the model field in Codex's TOML config text
export const setCodexModelName = (
  configText: string,
  modelName: string,
): string => {
  const trimmed = modelName.trim();
  const normalizedText = normalizeTomlText(configText);
  const lines = normalizedText ? normalizedText.split("\n") : [];
  const topLevelEndIndex = getTopLevelEndIndex(lines);
  const modelLineIndex = findTopLevelModelLineIndex(lines, topLevelEndIndex);

  if (!trimmed) {
    if (!normalizedText) return normalizedText;
    if (modelLineIndex !== -1) {
      lines.splice(modelLineIndex, 1);
    }
    return finalizeTomlText(lines);
  }

  // The model name may come from a remote /models response (dropdown
  // selection), so it must be escaped — a raw interpolation would let an id
  // containing quotes/control characters corrupt or even inject into
  // config.toml (e.g. forging [mcp_servers.*])
  const replacementLine = `model = ${tomlBasicString(trimmed)}`;
  if (modelLineIndex !== -1) {
    lines[modelLineIndex] = replacementLine;
    return finalizeTomlText(lines);
  }

  const modelProviderIndex = getTopLevelModelProviderLineIndex(lines);
  if (modelProviderIndex !== -1) {
    lines.splice(modelProviderIndex + 1, 0, replacementLine);
    return finalizeTomlText(lines);
  }

  if (lines.length === 0) {
    return `${replacementLine}\n`;
  }

  lines.splice(topLevelEndIndex, 0, replacementLine);
  return finalizeTomlText(lines);
};

// ========== Codex top-level integer field utils ==========

const tomlTopLevelIntPattern = (field: string) =>
  new RegExp(`^\\s*${field}\\s*=\\s*(\\d+)\\s*(?:#.*)?$`);

const findTopLevelIntMatch = (
  lines: string[],
  fieldName: string,
  topLevelEndIndex: number,
): { index: number; value: number } | undefined => {
  const pattern = tomlTopLevelIntPattern(fieldName);
  for (let i = 0; i < topLevelEndIndex; i += 1) {
    const m = lines[i].match(pattern);
    if (m) {
      return { index: i, value: Number(m[1]) };
    }
  }
  return undefined;
};

// Extract a top-level integer field from Codex's TOML config
export const extractCodexTopLevelInt = (
  configText: string | undefined | null,
  fieldName: string,
): number | undefined => {
  try {
    const raw = typeof configText === "string" ? configText : "";
    const text = normalizeTomlText(raw);
    if (!text) return undefined;
    const lines = text.split("\n");
    return findTopLevelIntMatch(lines, fieldName, getTopLevelEndIndex(lines))
      ?.value;
  } catch {
    return undefined;
  }
};

// Set or update a top-level integer field in Codex's TOML config
export const setCodexTopLevelInt = (
  configText: string,
  fieldName: string,
  value: number,
): string => {
  const normalizedText = normalizeTomlText(configText);
  const lines = normalizedText ? normalizedText.split("\n") : [];
  const topLevelEndIndex = getTopLevelEndIndex(lines);
  const existing = findTopLevelIntMatch(lines, fieldName, topLevelEndIndex);
  const replacementLine = `${fieldName} = ${value}`;

  if (existing) {
    lines[existing.index] = replacementLine;
    return finalizeTomlText(lines);
  }

  // Insertion point: end of the top-level region (before any section header)
  if (lines.length === 0) {
    return `${replacementLine}\n`;
  }

  lines.splice(topLevelEndIndex, 0, replacementLine);
  return finalizeTomlText(lines);
};

// Remove a top-level field line from Codex's TOML config
export const removeCodexTopLevelField = (
  configText: string,
  fieldName: string,
): string => {
  const normalizedText = normalizeTomlText(configText);
  if (!normalizedText) return normalizedText;
  const lines = normalizedText.split("\n");
  const topLevelEndIndex = getTopLevelEndIndex(lines);
  const existing = findTopLevelIntMatch(lines, fieldName, topLevelEndIndex);
  if (existing) {
    lines.splice(existing.index, 1);
  }
  return finalizeTomlText(lines);
};
