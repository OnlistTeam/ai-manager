import { describe, expect, it } from "vitest";
import {
  EMPTY_MCP_INSTALL_VALUES,
  mcpDraftFrom,
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

  it("keeps secret-bearing fields outside the generated draft", () => {
    const draft = mcpDraftFrom({
      ...EMPTY_MCP_INSTALL_VALUES,
      name: "Remote",
      transport: "http",
      url: "https://mcp.example.test",
    });
    const serialized = JSON.stringify(draft).toLocaleLowerCase();
    expect(serialized).not.toContain("headers");
    expect(serialized).not.toContain("environment");
    expect(serialized).not.toContain("token");
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
