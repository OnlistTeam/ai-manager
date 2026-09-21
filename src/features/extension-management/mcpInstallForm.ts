import type { McpInstallDraft } from "@/native";

export type McpTransport = "stdio" | "http" | "sse";

export interface McpInstallValues {
  name: string;
  description: string;
  transport: McpTransport;
  command: string;
  argumentsText: string;
  url: string;
}

export interface McpInstallErrors {
  name?: string;
  description?: string;
  command?: string;
  argumentsText?: string;
  url?: string;
}

export const EMPTY_MCP_INSTALL_VALUES: McpInstallValues = {
  name: "",
  description: "",
  transport: "stdio",
  command: "",
  argumentsText: "",
  url: "",
};

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
  } else {
    const error = urlError(values.url);
    if (error) errors.url = error;
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
      },
    };
  }
  return {
    ...shared,
    connection: {
      transport: values.transport,
      url: values.url.trim(),
    },
  };
}
