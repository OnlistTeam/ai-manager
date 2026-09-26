import { describe, expect, it } from "vitest";
import {
  EMPTY_MCP_INSTALL_VALUES,
  mcpDraftFrom,
  mcpVariableRow,
  validateMcpInstall,
} from "@/features/extension-management/mcpInstallForm";

describe("guided MCP install form", () => {
  it("builds a small local draft with one argument per line", () => {
    const values = {
      ...EMPTY_MCP_INSTALL_VALUES,
      name: " Files ",
      description: " Read selected files. ",
      command: " npx ",
      argumentsText: "-y\n\nserver-filesystem ",
    };
    expect(validateMcpInstall(values)).toEqual({});
    expect(mcpDraftFrom(values)).toEqual({
      name: "Files",
      description: "Read selected files.",
      connection: {
        transport: "stdio",
        command: "npx",
        arguments: ["-y", "server-filesystem"],
        env: [],
      },
    });
  });

  it("accepts secure remote URLs and loopback HTTP", () => {
    for (const url of [
      "https://mcp.example.test/v1",
      "http://localhost:3000/mcp",
      "http://127.0.0.1:4000/mcp",
      "http://[::1]:5000/mcp",
    ]) {
      expect(
        validateMcpInstall({
          ...EMPTY_MCP_INSTALL_VALUES,
          name: "Remote",
          transport: "http",
          url,
        }),
      ).toStrictEqual({});
    }
  });

  it("rejects public HTTP, embedded credentials, and fragments", () => {
    const errorFor = (url: string) =>
      validateMcpInstall({
        ...EMPTY_MCP_INSTALL_VALUES,
        name: "Remote",
        transport: "sse",
        url,
      }).url;
    expect(errorFor("http://mcp.example.test/events")).toBe(
      "error.mcp.urlInsecure",
    );
    expect(errorFor("https://user:secret@mcp.example.test")).toBe(
      "error.mcp.urlCredentials",
    );
    expect(errorFor("https://mcp.example.test/#token")).toBe(
      "error.mcp.urlInvalid",
    );
  });

  it("sends env for a local command and headers for a remote one, skipping blank rows", () => {
    const env = [
      mcpVariableRow(" GITHUB_TOKEN ", " ghp_example "),
      mcpVariableRow("", ""),
    ];
    const headers = [mcpVariableRow("Authorization", "Bearer abc")];
    const local = mcpDraftFrom({
      ...EMPTY_MCP_INSTALL_VALUES,
      name: "GitHub",
      command: "npx",
      env,
      headers,
    });
    expect(local.connection).toEqual({
      transport: "stdio",
      command: "npx",
      arguments: [],
      env: [{ name: "GITHUB_TOKEN", value: "ghp_example" }],
    });

    const remote = mcpDraftFrom({
      ...EMPTY_MCP_INSTALL_VALUES,
      name: "Remote",
      transport: "http",
      url: "https://mcp.example.test",
      env,
      headers,
    });
    expect(remote.connection).toEqual({
      transport: "http",
      url: "https://mcp.example.test",
      headers: [{ name: "Authorization", value: "Bearer abc" }],
    });
    expect(JSON.stringify(remote)).not.toContain("GITHUB_TOKEN");
  });

  it("checks variable names, repeats, and single-line values without echoing them", () => {
    const envError = (...rows: Array<[string, string]>) =>
      validateMcpInstall({
        ...EMPTY_MCP_INSTALL_VALUES,
        name: "Local",
        command: "npx",
        env: rows.map(([name, value]) => mcpVariableRow(name, value)),
      }).env;
    expect(envError(["API_KEY", "secret"], ["_DEBUG", ""])).toBeUndefined();
    expect(envError(["", "orphan value"])).toBe("error.mcp.envInvalid");
    expect(envError(["1KEY", "x"])).toBe("error.mcp.envInvalid");
    expect(envError(["MY-KEY", "x"])).toBe("error.mcp.envInvalid");
    expect(envError(["KEY", "a"], ["KEY", "b"])).toBe("error.mcp.envInvalid");
    expect(envError(["KEY", "line\nbreak"])).toBe("error.mcp.envInvalid");

    const headerError = (...rows: Array<[string, string]>) =>
      validateMcpInstall({
        ...EMPTY_MCP_INSTALL_VALUES,
        name: "Remote",
        transport: "sse",
        url: "https://mcp.example.test",
        headers: rows.map(([name, value]) => mcpVariableRow(name, value)),
      }).headers;
    expect(headerError(["X-Api-Key", "abc"])).toBeUndefined();
    expect(headerError(["X Api Key", "abc"])).toBe("error.mcp.headersInvalid");
    expect(headerError(["Authorization", "a"], ["authorization", "b"])).toBe(
      "error.mcp.headersInvalid",
    );
  });

  it("rejects C0 and C1 control characters before native validation", () => {
    expect(
      validateMcpInstall({
        ...EMPTY_MCP_INSTALL_VALUES,
        name: "Remote\u0085name",
        transport: "http",
        url: "https://mcp.example.test",
      }).name,
    ).toBe("error.mcp.nameInvalid");
  });
});
