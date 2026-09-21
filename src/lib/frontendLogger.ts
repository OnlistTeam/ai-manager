import { error as writeErrorLog } from "@tauri-apps/plugin-log";

const MAX_LOG_MESSAGE_LENGTH = 12_000;
const MAX_RAW_LOG_INPUT_LENGTH = 16_000;
const MAX_SERIALIZED_STRING_LENGTH = 2_000;
const MAX_SERIALIZED_ENTRIES = 32;
const MAX_SERIALIZED_TOTAL_VALUES = 64;
const MAX_SERIALIZATION_DEPTH = 4;
const QUERY_VALUE_PATTERN = /([?&][A-Za-z0-9_.~-]+)=([^&#\s"'<>]*)/g;
const URL_CREDENTIAL_PATTERN = /(https?:\/\/)[^/@\s]+@/gi;
const QUOTED_NAMED_SECRET_PATTERN =
  /((?:api[_-]?key|access[_-]?token|refresh[_-]?token|token|authorization|auth|password|passwd|pwd|secret|cookie)\s*["']?\s*[:=]\s*)(["'])(.*?)\2/gi;
const NAMED_SECRET_PATTERN =
  /((?:api[_-]?key|access[_-]?token|refresh[_-]?token|token|authorization|auth|password|passwd|pwd|secret|cookie)\s*["']?\s*[:=]\s*["']?)([^\s"',}]+)/gi;
// When a sensitive key's value is an array/object (`"tokens":[...]`, `"auth":{...}`),
// the scalar regexes above can't reach the elements inside. The text layer is the single
// exit point where every entry (Error/string/object/nested/prefix+JSON) ultimately
// converges, so this is the fallback: once a sensitive key name is matched, replace the
// following `[..]`/`{..}` as a whole. `\b` prevents matching suffixes like "monkey";
// `(?:\\?["'])?` handles both bare and escaped quotes (e.g. `\"tokens\"` in double-encoded JSON).
const NAMED_SECRET_CONTAINER_PATTERN =
  /((?:\\?["'])?\b(?:api[_-]?key|access[_-]?key|secret[_-]?key|private[_-]?key|client[_-]?secret|auth[_-]?token|access[_-]?token|refresh[_-]?token|id[_-]?token|session[_-]?token|session[_-]?id|authorization|credential|password|passwd|bearer|cookie|secret|token|auth|pwd|key)s?(?:\\?["'])?\s*[:=]\s*)(\[[^\]]*\]|\{[^{}]*\})/gi;
const SENSITIVE_HEADER_LINE_PATTERN =
  /(^|[\r\n])([ \t]*(?:(?:proxy-)?authorization|cookie|set-cookie|x-api-key|api-key)\s*[:=]\s*)[^\r\n]+/gim;
const AUTH_SCHEME_PATTERN =
  /\b(Bearer|Basic|Token|ApiKey|Digest|Negotiate|AWS4-HMAC-SHA256)\s+[^\s"',}\]]+/gi;
const SECRET_VALUE_IN_TEXT_PATTERN =
  /(^|[^A-Za-z0-9]|\\[nrt])(?:sk-[A-Za-z0-9._~+/-]{6,}|AIza[A-Za-z0-9_-]{8,}|github_pat_[A-Za-z0-9_]{6,}|gh[pousr]_[A-Za-z0-9_]{6,}|xox[baprs]-[A-Za-z0-9-]{6,}|ya29\.[A-Za-z0-9._-]{6,}|(?:AKIA|ASIA)[A-Z0-9]{12,}|eyJ[A-Za-z0-9_-]+\.[A-Za-z0-9_-]+\.[A-Za-z0-9_-]+)/gim;
const SECRET_VALUE_WITHIN_STRING_PATTERN =
  /(?:sk-[A-Za-z0-9._~+/-]{6,}|AIza[A-Za-z0-9_-]{8,}|github_pat_[A-Za-z0-9_]{6,}|gh[pousr]_[A-Za-z0-9_]{6,}|xox[baprs]-[A-Za-z0-9-]{6,}|ya29\.[A-Za-z0-9._-]{6,}|(?:AKIA|ASIA)[A-Z0-9]{12,}|eyJ[A-Za-z0-9_-]+\.[A-Za-z0-9_-]+\.[A-Za-z0-9_-]+)/i;
function truncateForProcessing(input: string, limit: number): string {
  if (input.length <= limit) {
    return input;
  }
  return `${input.slice(0, limit)}\n[input truncated]`;
}

export function redactFrontendLogText(input: string): string {
  return input
    .replace(QUERY_VALUE_PATTERN, "$1=[REDACTED]")
    .replace(URL_CREDENTIAL_PATTERN, "$1[REDACTED]@")
    .replace(SENSITIVE_HEADER_LINE_PATTERN, "$1$2[REDACTED]")
    .replace(AUTH_SCHEME_PATTERN, "$1 [REDACTED]")
    .replace(SECRET_VALUE_IN_TEXT_PATTERN, "$1[REDACTED]")
    .replace(NAMED_SECRET_CONTAINER_PATTERN, "$1[REDACTED]")
    .replace(QUOTED_NAMED_SECRET_PATTERN, "$1$2[REDACTED]$2")
    .replace(NAMED_SECRET_PATTERN, "$1[REDACTED]");
}

function looksLikeSecretValue(value: string): boolean {
  const trimmed = value.trim();
  if (SECRET_VALUE_WITHIN_STRING_PATTERN.test(trimmed)) {
    return true;
  }
  if (/-----BEGIN (?:RSA |EC |OPENSSH )?PRIVATE KEY-----/i.test(trimmed)) {
    return true;
  }

  // Unknown opaque credentials: keep this only in the structured serializer,
  // where a false positive costs diagnostics but cannot alter application data.
  return (
    trimmed.length >= 32 &&
    /^[A-Za-z0-9._~+/=-]+$/.test(trimmed) &&
    /[A-Za-z]/.test(trimmed) &&
    /\d/.test(trimmed)
  );
}

// In structured objects, matching one of these property names means the entire value
// (scalar/array/object) is treated as sensitive and hidden wholesale. Names are stored in
// singular form, and the trailing `s` is stripped before lookup so plurals
// (tokens/apiKeys/credentials) are covered automatically without enumerating every case —
// the text-layer regex above can only match scalar `"name":"value"` pairs, not arrays/nesting.
const SENSITIVE_KEY_NAMES = new Set([
  "key",
  "apikey",
  "accesskey",
  "secretkey",
  "privatekey",
  "clientsecret",
  "token",
  "authtoken",
  "accesstoken",
  "refreshtoken",
  "idtoken",
  "sessiontoken",
  "sessionid",
  "authorization",
  "auth",
  "bearer",
  "password",
  "passwd",
  "pwd",
  "secret",
  "credential",
  "cookie",
]);

function isSensitiveKey(key: string): boolean {
  const normalized = key
    .toLowerCase()
    .replace(/[^a-z0-9]/g, "")
    .replace(/s$/, "");
  return SENSITIVE_KEY_NAMES.has(normalized);
}

function normalizeForSerialization(
  value: unknown,
  depth: number,
  ancestors: WeakSet<object>,
  budget: { remaining: number },
): unknown {
  if (budget.remaining <= 0) {
    return "[Serialization budget exhausted]";
  }
  budget.remaining -= 1;

  if (typeof value === "string") {
    // Value-level redaction: hide any opaque string that "looks like a secret" wholesale.
    // Named fields (apiKey/token/...) are already hidden wholesale by isSensitiveKey above
    // based on property name; bare secrets embedded in text are caught by
    // redactFrontendLogText as a fallback.
    if (looksLikeSecretValue(value)) {
      return "[REDACTED]";
    }
    return truncateForProcessing(value, MAX_SERIALIZED_STRING_LENGTH);
  }
  if (
    value == null ||
    typeof value === "number" ||
    typeof value === "boolean"
  ) {
    return value;
  }
  if (typeof value === "bigint") {
    return `${value}n`;
  }
  if (typeof value === "symbol") {
    return String(value);
  }
  if (typeof value === "function") {
    return `[Function ${value.name || "anonymous"}]`;
  }
  if (typeof value !== "object") {
    return String(value);
  }
  if (ancestors.has(value)) {
    return "[Circular]";
  }
  if (depth >= MAX_SERIALIZATION_DEPTH) {
    return "[Object: max depth reached]";
  }

  ancestors.add(value);
  try {
    if (Array.isArray(value)) {
      const items: unknown[] = [];
      for (const item of value.slice(0, MAX_SERIALIZED_ENTRIES)) {
        if (budget.remaining <= 0) {
          break;
        }
        items.push(
          normalizeForSerialization(item, depth + 1, ancestors, budget),
        );
      }
      if (value.length > items.length) {
        items.push(`[${value.length - items.length} more items]`);
      }
      return items;
    }

    const output: Record<string, unknown> = {};
    const keys = Object.keys(value);
    for (const key of keys.slice(0, MAX_SERIALIZED_ENTRIES)) {
      if (budget.remaining <= 0) {
        output["[truncated]"] = "Serialization budget exhausted";
        break;
      }
      try {
        const descriptor = Object.getOwnPropertyDescriptor(value, key);
        if (descriptor?.get) {
          output[key] = "[Getter omitted]";
          continue;
        }
        if (isSensitiveKey(key)) {
          // Sensitive property name -> hide the entire value (including arrays/objects)
          // unconditionally, without recursing or guessing its shape.
          output[key] = "[REDACTED]";
          continue;
        }
        output[key] = normalizeForSerialization(
          descriptor?.value,
          depth + 1,
          ancestors,
          budget,
        );
      } catch {
        output[key] = "[Property access failed]";
      }
    }
    if (keys.length > MAX_SERIALIZED_ENTRIES) {
      output["[truncated]"] =
        `${keys.length - MAX_SERIALIZED_ENTRIES} more properties`;
    }
    return output;
  } finally {
    ancestors.delete(value);
  }
}

function serializeStructured(value: unknown): string | null {
  try {
    const serialized = JSON.stringify(
      normalizeForSerialization(value, 0, new WeakSet(), {
        remaining: MAX_SERIALIZED_TOTAL_VALUES,
      }),
    );
    return serialized ?? null;
  } catch {
    return null;
  }
}

// Structured data can arrive disguised as "JSON serialized to a string"
// (Promise.reject(JSON.stringify(...)), throw new Error(JSON.stringify(...))). Without
// reparsing it back into structure, only the text regexes apply, which can't reach
// array/nested fields. If it looks like JSON, parse it and redact at the property level;
// otherwise return null and let the caller fall back to plain-text handling.
function redactStructuredString(text: string): string | null {
  const trimmed = text.trim();
  if (!(trimmed.startsWith("{") || trimmed.startsWith("["))) {
    return null;
  }
  if (text.length > MAX_RAW_LOG_INPUT_LENGTH) {
    // Truncating otherwise-valid oversized JSON would always produce invalid JSON, which
    // would fall back to the text regexes that can't reach array fields and would leak
    // data; parsing multi-MiB input and risking blocking the UI isn't worth it either.
    // If it looks like JSON and exceeds the limit, discard it wholesale.
    return "[oversized structured error omitted]";
  }
  let parsed: unknown;
  try {
    parsed = JSON.parse(trimmed);
  } catch {
    return null;
  }
  // Scalar JSON (e.g. "42") has no fields to redact; hand it to the text layer.
  if (parsed === null || typeof parsed !== "object") {
    return null;
  }
  return serializeStructured(parsed) ?? "[Unserializable structured error]";
}

// Renders an Error as "redacted message + native stack trace", independent of the
// browser engine's stack format:
//  - V8/Chromium (Windows WebView2): the message is embedded in the stack's first line ->
//    replace that literal string globally with the redacted version;
//  - WebKit/JSC (macOS/Linux WKWebView), SpiderMonkey: the stack is pure frames and does
//    not contain the message -> prepend a redacted message header.
// This deliberately does not try to recognize engine-specific frame formats like `    at `
// or `@` (an incomplete enumeration is exactly what used to drop the whole WebKit stack).
// Instead it branches purely on "does the message appear inside the stack", so the native
// stack is preserved on every platform and an unredacted message never leaks through.
function renderRedactedError(error: Error, structuredMessage: string): string {
  const head = `${error.name}: ${structuredMessage}`;
  const stack = error.stack;
  if (!stack) {
    return head;
  }
  if (error.message && stack.includes(error.message)) {
    // V8: message is embedded in the stack -- replace the literal string globally
    // (split/join swaps out every occurrence), leaving the frames untouched.
    return stack.split(error.message).join(structuredMessage);
  }
  // WebKit/Firefox: the stack doesn't contain the message -- prepend a redacted message
  // header to the raw frames.
  return `${head}\n${stack}`;
}

function describeError(error: unknown): string {
  if (error instanceof Error) {
    // `throw new Error(JSON.stringify(payload))` is common: credentials can end up
    // hiding in the message, and on V8 the stack's first line is the raw message. First
    // try structured JSON redaction on the message; if it hits, render it as "redacted
    // message + native stack" so the stack doesn't leak the unredacted message directly.
    const structuredMessage = redactStructuredString(error.message);
    if (structuredMessage !== null) {
      return truncateForProcessing(
        renderRedactedError(error, structuredMessage),
        MAX_RAW_LOG_INPUT_LENGTH,
      );
    }
    return truncateForProcessing(
      error.stack || `${error.name}: ${error.message}`,
      MAX_RAW_LOG_INPUT_LENGTH,
    );
  }
  if (typeof error === "string") {
    const structured = redactStructuredString(error);
    if (structured !== null) {
      return structured;
    }
    return truncateForProcessing(error, MAX_RAW_LOG_INPUT_LENGTH);
  }
  if (error == null) {
    return String(error);
  }
  const structured = serializeStructured(error);
  if (structured === null) {
    return "[Unserializable thrown value]";
  }
  return truncateForProcessing(structured, MAX_RAW_LOG_INPUT_LENGTH);
}

export function reportFrontendError(
  context: string,
  error: unknown,
  details?: string,
): void {
  // Cap each raw input segment first, then run the global regexes, so an exception
  // carrying multi-MiB text can't block the UI.
  const raw = truncateForProcessing(
    [
      `[frontend] ${truncateForProcessing(context, MAX_RAW_LOG_INPUT_LENGTH)}`,
      describeError(error),
      details
        ? truncateForProcessing(details, MAX_RAW_LOG_INPUT_LENGTH).trim()
        : undefined,
    ]
      .filter(Boolean)
      .join("\n"),
    MAX_RAW_LOG_INPUT_LENGTH,
  );
  const redacted = redactFrontendLogText(raw);
  const message =
    redacted.length > MAX_LOG_MESSAGE_LENGTH
      ? `${redacted.slice(0, MAX_LOG_MESSAGE_LENGTH)}\n[truncated]`
      : redacted;

  // In web dev/test environments there's no Tauri invoke; a failed log report must not
  // trigger console.error or an unhandled Promise, or it would create an error loop.
  void writeErrorLog(message, { file: "frontend" }).catch(() => undefined);
}

export function installGlobalErrorHandlers(
  target: Window = window,
): () => void {
  const handleError = (event: ErrorEvent) => {
    const location = event.filename
      ? `${event.filename}:${event.lineno}:${event.colno}`
      : undefined;
    reportFrontendError("window.error", event.error ?? event.message, location);
  };
  const handleUnhandledRejection = (event: PromiseRejectionEvent) => {
    reportFrontendError("unhandledrejection", event.reason);
  };

  target.addEventListener("error", handleError);
  target.addEventListener("unhandledrejection", handleUnhandledRejection);

  return () => {
    target.removeEventListener("error", handleError);
    target.removeEventListener("unhandledrejection", handleUnhandledRejection);
  };
}
