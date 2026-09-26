import type { McpInstallDraft } from "@/native";

export type McpTransport = "stdio" | "http" | "sse";

/** Environment variables for a local command, request headers for a remote one. */
export type McpVariableKind = "env" | "headers";

export interface McpVariableRow {
  /** Stable React key only; never sent to native. */
  id: string;
  name: string;
  value: string;
}

export interface McpInstallValues {
  name: string;
  description: string;
  transport: McpTransport;
  command: string;
  argumentsText: string;
  env: McpVariableRow[];
  url: string;
  headers: McpVariableRow[];
}

export type McpTextField = Exclude<
  keyof McpInstallValues,
  McpVariableKind | "transport"
>;

export interface McpInstallErrors {
  name?: string;
  description?: string;
  command?: string;
  argumentsText?: string;
  env?: string;
  url?: string;
  headers?: string;
}

export const EMPTY_MCP_INSTALL_VALUES: McpInstallValues = {
  name: "",
  description: "",
  transport: "stdio",
  command: "",
  argumentsText: "",
  env: [],
  url: "",
  headers: [],
};

const MAX_VARIABLES = 64;
const MAX_VARIABLE_NAME_CHARS = 128;
const MAX_VARIABLE_VALUE_CHARS = 8_192;
const ENV_NAME = /^[A-Za-z_][A-Za-z0-9_]*$/;
// RFC 9110 `token`.
const HEADER_NAME = /^[!#$%&'*+\-.^_`|~0-9A-Za-z]+$/;

let nextRowId = 0;

export function mcpVariableRow(name = "", value = ""): McpVariableRow {
  nextRowId += 1;
  return { id: `mcp-variable-${nextRowId}`, name, value };
}

function count(value: string): number {
  return Array.from(value).length;
}

function containsControl(value: string): boolean {
  return Array.from(value).some((character) => {
    const code = character.codePointAt(0) ?? 0;
    return code < 32 || (code >= 127 && code <= 159);
  });
}

function argumentsFrom(text: string): string[] {
  return text
    .split("\n")
    .map((value) => value.trim())
    .filter(Boolean);
}

/** A row left completely blank is an unused slot, not an error. */
function variablesFrom(rows: McpVariableRow[]) {
  return rows
    .map((row) => ({ name: row.name.trim(), value: row.value.trim() }))
    .filter((row) => row.name || row.value);
}

/** Values are often API keys: the result is a message key and never echoes one. */
function variablesError(
  rows: McpVariableRow[],
  kind: McpVariableKind,
): string | undefined {
  const error =
    kind === "env" ? "error.mcp.envInvalid" : "error.mcp.headersInvalid";
  const pattern = kind === "env" ? ENV_NAME : HEADER_NAME;
  const variables = variablesFrom(rows);
  if (variables.length > MAX_VARIABLES) return error;

  const seen = new Set<string>();
  for (const { name, value } of variables) {
    if (count(name) > MAX_VARIABLE_NAME_CHARS || !pattern.test(name)) {
      return error;
    }
    if (count(value) > MAX_VARIABLE_VALUE_CHARS || containsControl(value)) {
      return error;
    }
    const identity = kind === "env" ? name : name.toLowerCase();
    if (seen.has(identity)) return error;
    seen.add(identity);
  }
  return undefined;
}

function loopback(hostname: string): boolean {
  const host = hostname.toLocaleLowerCase();
  return (
    host === "localhost" ||
    host.endsWith(".localhost") ||
    host === "[::1]" ||
    host === "::1" ||
    host.startsWith("127.")
  );
}

function urlError(raw: string): string | undefined {
  const value = raw.trim();
  if (!value) return "error.mcp.urlRequired";
  if (value.length > 2_048 || containsControl(value)) {
    return "error.mcp.urlInvalid";
  }
  let parsed: URL;
  try {
    parsed = new URL(value);
  } catch {
    return "error.mcp.urlInvalid";
  }
  if (!parsed.hostname || parsed.hash) return "error.mcp.urlInvalid";
  if (parsed.username || parsed.password) return "error.mcp.urlCredentials";
  if (parsed.protocol === "https:") return undefined;
  if (parsed.protocol === "http:" && loopback(parsed.hostname))
    return undefined;
  return parsed.protocol === "http:"
    ? "error.mcp.urlInsecure"
    : "error.mcp.urlInvalid";
}

export function validateMcpInstall(values: McpInstallValues): McpInstallErrors {
  const errors: McpInstallErrors = {};
  const name = values.name.trim();
  if (!name) errors.name = "error.mcp.nameRequired";
  else if (count(name) > 80) errors.name = "error.mcp.nameTooLong";
  else if (containsControl(name)) errors.name = "error.mcp.nameInvalid";

  if (count(values.description.trim()) > 240) {
    errors.description = "error.mcp.descriptionTooLong";
  }

  if (values.transport === "stdio") {
    const command = values.command.trim();
    const args = argumentsFrom(values.argumentsText);
    if (!command) errors.command = "error.mcp.commandRequired";
    else if (count(command) > 512 || containsControl(command)) {
      errors.command = "error.mcp.commandInvalid";
    }
    if (
      args.length > 64 ||
      args.some(
        (argument) => count(argument) > 2_048 || containsControl(argument),
      )
    ) {
      errors.argumentsText = "error.mcp.argumentsInvalid";
    }
    const env = variablesError(values.env, "env");
    if (env) errors.env = env;
  } else {
    const error = urlError(values.url);
    if (error) errors.url = error;
    const headers = variablesError(values.headers, "headers");
    if (headers) errors.headers = headers;
  }
  return errors;
}

export function mcpDraftFrom(values: McpInstallValues): McpInstallDraft {
  const shared = {
    name: values.name.trim(),
    description: values.description.trim() || null,
  };
  if (values.transport === "stdio") {
    return {
      ...shared,
      connection: {
        transport: "stdio",
        command: values.command.trim(),
        arguments: argumentsFrom(values.argumentsText),
        env: variablesFrom(values.env),
      },
    };
  }
  return {
    ...shared,
    connection: {
      transport: values.transport,
      url: values.url.trim(),
      headers: variablesFrom(values.headers),
    },
  };
}
