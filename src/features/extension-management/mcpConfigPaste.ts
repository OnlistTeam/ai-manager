import {
  mcpVariableRow,
  type McpInstallValues,
  type McpVariableRow,
} from "./mcpInstallForm";

/**
 * Turns an MCP configuration pasted from a README into form fields. The JSON
 * never leaves the renderer: the user reviews the filled fields and submits
 * the typed draft as usual (ADR-0047).
 *
 * Accepted shapes:
 * - `{ "mcpServers": { "<name>": { ... } } }` (also `servers`, `mcp`)
 * - `{ "<name>": { ... } }` and the bare fragment `"<name>": { ... }`
 * - a single server `{ "command": ... }` or `{ "type": "http", "url": ... }`
 *
 * When several servers are present only the first is used, and the caller
 * says so. One form describes one connection; asking the user to pick first
 * would add a step to the common single-server case.
 */

export type McpConnectionFields = Pick<
  McpInstallValues,
  "transport" | "command" | "argumentsText" | "env" | "url" | "headers"
>;

export interface McpPastedConnection {
  /** The server's key in the pasted JSON; null for a bare server object. */
  name: string | null;
  fields: McpConnectionFields;
  /** How many servers the paste contained; only the first was used. */
  total: number;
}

type JsonObject = Record<string, unknown>;

const WRAPPERS = ["mcpServers", "servers", "mcp"] as const;
const URL_KEYS = ["url", "serverUrl", "httpUrl"] as const;

function isObject(value: unknown): value is JsonObject {
  return typeof value === "object" && value !== null && !Array.isArray(value);
}

function scalar(value: unknown): string | null {
  if (typeof value === "string") return value;
  if (typeof value === "number" || typeof value === "boolean") {
    return String(value);
  }
  return null;
}

function scalars(value: unknown): string[] {
  if (!Array.isArray(value)) return [];
  return value.map(scalar).filter((item): item is string => item !== null);
}

function remoteUrl(spec: JsonObject): string | null {
  for (const key of URL_KEYS) {
    const value = spec[key];
    if (typeof value === "string" && value.trim()) return value.trim();
  }
  return null;
}

function commandOf(spec: JsonObject): string[] {
  const command = spec.command;
  if (typeof command === "string") return command.trim() ? [command] : [];
  // OpenCode writes the program and its arguments as one array.
  return scalars(command);
}

function isServer(value: unknown): value is JsonObject {
  return isObject(value) && (commandOf(value).length > 0 || !!remoteUrl(value));
}

function rows(value: unknown): McpVariableRow[] {
  if (!isObject(value)) return [];
  return Object.entries(value).flatMap(([name, raw]) => {
    const text = scalar(raw);
    return text === null ? [] : [mcpVariableRow(name, text)];
  });
}

function fieldsFrom(spec: JsonObject): McpConnectionFields {
  const kind = scalar(spec.type ?? spec.transport)?.toLowerCase();
  const url = remoteUrl(spec);
  const command = commandOf(spec);
  const empty = {
    command: "",
    argumentsText: "",
    env: [],
    url: "",
    headers: [],
  };
  // Like upstream, a server without a type is a local command.
  const remote = kind !== undefined && kind !== "stdio" && kind !== "local";

  if (url && (command.length === 0 || remote)) {
    return {
      ...empty,
      transport: kind === "sse" ? "sse" : "http",
      url,
      headers: rows(spec.headers),
    };
  }
  const [program, ...inline] = command;
  return {
    ...empty,
    transport: "stdio",
    command: program.trim(),
    argumentsText: [...inline, ...scalars(spec.args)].join("\n"),
    env: rows(spec.env ?? spec.environment),
  };
}

function parseObject(text: string): JsonObject | null {
  let source = text.trim();
  // A fragment copied out of `mcpServers`: `"github": { ... },`
  if (source.startsWith('"')) source = `{${source.replace(/,\s*$/, "")}}`;
  try {
    const parsed: unknown = JSON.parse(source);
    return isObject(parsed) ? parsed : null;
  } catch {
    return null;
  }
}

function serversIn(root: JsonObject): Array<[string | null, JsonObject]> {
  if (isServer(root)) return [[null, root]];
  const wrapper = WRAPPERS.map((key) => root[key]).find(
    (value) => isObject(value) && !isServer(value),
  );
  const container = isObject(wrapper) ? wrapper : root;
  return Object.entries(container).filter(
    (entry): entry is [string, JsonObject] => isServer(entry[1]),
  );
}

export function parseMcpConfig(text: string): McpPastedConnection | null {
  const root = parseObject(text);
  if (!root) return null;
  const servers = serversIn(root);
  if (servers.length === 0) return null;
  const [name, spec] = servers[0];
  return {
    name: name?.trim() || null,
    fields: fieldsFrom(spec),
    total: servers.length,
  };
}
