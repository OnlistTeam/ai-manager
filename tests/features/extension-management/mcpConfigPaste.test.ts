import { describe, expect, it } from "vitest";
import { parseMcpConfig } from "@/features/extension-management/mcpConfigPaste";

function variables(rows: Array<{ name: string; value: string }> | undefined) {
  return rows?.map(({ name, value }) => ({ name, value }));
}

describe("parseMcpConfig", () => {
  it("reads a README mcpServers block into local command fields", () => {
    const pasted = parseMcpConfig(`{
      "mcpServers": {
        "github": {
          "command": "npx",
          "args": ["-y", "@modelcontextprotocol/server-github"],
          "env": { "GITHUB_TOKEN": "<YOUR_TOKEN>", "PORT": 8080 }
        }
      }
    }`);
    expect(pasted?.name).toBe("github");
    expect(pasted?.total).toBe(1);
    expect(pasted?.fields).toMatchObject({
      transport: "stdio",
      command: "npx",
      argumentsText: "-y\n@modelcontextprotocol/server-github",
      url: "",
      headers: [],
    });
    expect(variables(pasted?.fields.env)).toEqual([
      { name: "GITHUB_TOKEN", value: "<YOUR_TOKEN>" },
      { name: "PORT", value: "8080" },
    ]);
  });

  it("reads a bare remote server with headers and keeps the current name", () => {
    const pasted = parseMcpConfig(
      '{"type":"sse","url":"https://mcp.example.test/sse","headers":{"Authorization":"Bearer abc"}}',
    );
    expect(pasted?.name).toBeNull();
    expect(pasted?.fields).toMatchObject({
      transport: "sse",
      url: "https://mcp.example.test/sse",
      command: "",
      argumentsText: "",
      env: [],
    });
    expect(variables(pasted?.fields.headers)).toEqual([
      { name: "Authorization", value: "Bearer abc" },
    ]);
  });

  it("treats an untyped or streamable URL server as HTTP", () => {
    for (const text of [
      '{"docs":{"url":"https://mcp.example.test/v1"}}',
      '{"docs":{"type":"streamable-http","url":"https://mcp.example.test/v1"}}',
      '{"docs":{"serverUrl":"https://mcp.example.test/v1"}}',
    ]) {
      const pasted = parseMcpConfig(text);
      expect(pasted?.name, text).toBe("docs");
      expect(pasted?.fields.transport, text).toBe("http");
      expect(pasted?.fields.url, text).toBe("https://mcp.example.test/v1");
    }
  });

  it("accepts a fragment copied out of mcpServers, trailing comma included", () => {
    const pasted = parseMcpConfig(
      '"fetch": { "command": "uvx", "args": ["mcp-server-fetch"] },',
    );
    expect(pasted?.name).toBe("fetch");
    expect(pasted?.fields.command).toBe("uvx");
    expect(pasted?.fields.argumentsText).toBe("mcp-server-fetch");
  });

  it("splits an array command and reads OpenCode's environment key", () => {
    const pasted = parseMcpConfig(
      '{"mcp":{"files":{"type":"local","command":["npx","-y","server-files"],"environment":{"ROOT":"/tmp"}}}}',
    );
    expect(pasted?.name).toBe("files");
    expect(pasted?.fields.command).toBe("npx");
    expect(pasted?.fields.argumentsText).toBe("-y\nserver-files");
    expect(variables(pasted?.fields.env)).toEqual([
      { name: "ROOT", value: "/tmp" },
    ]);
  });

  it("uses the first of several servers and reports how many there were", () => {
    const pasted = parseMcpConfig(
      '{"servers":{"alpha":{"command":"a"},"beta":{"url":"https://b.example.test"}}}',
    );
    expect(pasted?.name).toBe("alpha");
    expect(pasted?.total).toBe(2);
    expect(pasted?.fields.command).toBe("a");
  });

  it("reads a server literally named mcp as a named server, not a wrapper", () => {
    const pasted = parseMcpConfig('{"mcp":{"command":"npx"}}');
    expect(pasted?.name).toBe("mcp");
    expect(pasted?.fields.command).toBe("npx");
  });

  it("returns null for anything that is not an MCP configuration", () => {
    for (const text of [
      "",
      "npx -y server",
      "[1, 2]",
      '{"mcpServers":{}}',
      '{"name":"no connection"}',
      '{"github":{"command":"  "}}',
      "{ broken",
    ]) {
      expect(parseMcpConfig(text), text).toBeNull();
    }
  });
});
