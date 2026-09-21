import { describe, expect, it } from "vitest";
import { aggregateQuickCheck } from "@/features/health";
import type { Tool } from "@/entities/tool";
import type { HealthSnapshot, LocalExtensionInventory } from "@/native";

const CAPABILITIES = {
  canInstall: true,
  canUpdate: true,
  canUninstall: true,
  canRepair: false,
  canManageProvider: true,
  canManageMcp: true,
  canManageSkills: true,
  canManagePrompts: true,
  canManageVersion: false,
  canLaunch: true,
};

function tool(id: Tool["id"], status: Tool["status"] = "installed"): Tool {
  return {
    id,
    name: id,
    descriptionKey: `tool.${id}.description`,
    status,
    version: status === "notInstalled" ? null : "1.0.0",
    latestVersion: status === "updateAvailable" ? "2.0.0" : null,
    capabilities: CAPABILITIES,
    sessionsInsideSettings: false,
    environment: "macos",
  };
}

function snapshot(overrides: Partial<HealthSnapshot> = {}): HealthSnapshot {
  return {
    providers: [],
    configs: [],
    mcp: { total: 0, enabled: 0 },
    ...overrides,
  };
}

describe("aggregateQuickCheck", () => {
  it("covers every tool state without treating not-installed as a failure", () => {
    const result = aggregateQuickCheck(
      [
        tool("claude-code"),
        tool("codex", "updateAvailable"),
        tool("opencode", "broken"),
        tool("gemini-cli", "unknown"),
        tool("claude-code", "notInstalled"),
      ],
      snapshot(),
      {},
    );
    expect(result.status).toBe("action");
    expect(result.attentionCount).toBe(3);
    expect(result.installedCount).toBe(2);
    expect(result.updatableCount).toBe(1);
    expect(result.configuredServiceCount).toBe(0);
    expect(result.mcpTotalCount).toBe(0);
    expect(result.mcpEnabledCount).toBe(0);

    const notInstalled = aggregateQuickCheck(
      [tool("claude-code", "notInstalled")],
      snapshot(),
      {},
    );
    expect(notInstalled.attentionCount).toBe(0);
    expect(notInstalled.status).toBe("attention");
    expect(notInstalled.items[0]?.status).toBe("info");
  });

  /**
   * `configured` counts services saved in this application and nothing else.
   * Every tool here can also authenticate with its own vendor login, so a tool
   * with none saved is usually a tool that works. It stays on the list, with
   * its offer to connect one, but it is information rather than a warning.
   */
  it("does not treat a tool with no saved service as a problem", () => {
    const result = aggregateQuickCheck(
      [tool("claude-code")],
      snapshot({
        providers: [
          {
            tool: "claude-code",
            configured: false,
            configuredCount: 0,
            checkTargets: [],
          },
        ],
        configs: [{ tool: "claude-code", status: "readable" }],
      }),
      {},
    );
    expect(result.attentionCount).toBe(0);
    expect(result.status).toBe("ready");
    expect(result.items.find((item) => item.kind === "provider")).toMatchObject(
      {
        status: "info",
        toolId: "claude-code",
        resolution: "connectService",
      },
    );
  });

  it("escalates a live configuration parse failure to action required", () => {
    const result = aggregateQuickCheck(
      [tool("claude-code")],
      snapshot({
        providers: [
          {
            tool: "claude-code",
            configured: true,
            configuredCount: 1,
            checkTargets: [],
          },
        ],
        configs: [{ tool: "claude-code", status: "unreadable" }],
      }),
      {},
    );
    expect(result.attentionCount).toBe(1);
    expect(result.status).toBe("action");
  });

  it("treats a live configuration that does not exist yet as information, not a problem", () => {
    const result = aggregateQuickCheck(
      [tool("gemini-cli")],
      snapshot({
        providers: [
          {
            tool: "gemini-cli",
            configured: true,
            configuredCount: 1,
            checkTargets: [],
          },
        ],
        configs: [{ tool: "gemini-cli", status: "missing" }],
      }),
      {},
    );
    expect(result.attentionCount).toBe(0);
    expect(result.status).toBe("ready");
    expect(result.items.find((item) => item.kind === "config")).toMatchObject({
      status: "info",
      titleKey: "preferences.check.item.configMissing.title",
      resolution: undefined,
    });
  });

  it("uses cached manual connectivity but never penalizes not-checked targets", () => {
    const result = aggregateQuickCheck(
      [tool("claude-code")],
      snapshot({
        providers: [
          {
            tool: "claude-code",
            configured: true,
            configuredCount: 1,
            checkTargets: [
              { providerId: "ok", name: "OK" },
              { providerId: "slow", name: "Slow" },
              { providerId: "down", name: "Down" },
              { providerId: "new", name: "New" },
            ],
          },
        ],
        configs: [{ tool: "claude-code", status: "readable" }],
      }),
      {
        "claude-code": {
          ok: {
            providerId: "ok",
            reachability: "operational",
            responseTimeMs: 120,
            httpStatus: 200,
          },
          slow: {
            providerId: "slow",
            reachability: "degraded",
            responseTimeMs: 7100,
            httpStatus: 200,
          },
          down: {
            providerId: "down",
            reachability: "failed",
            responseTimeMs: null,
            httpStatus: null,
          },
        },
      },
    );
    expect(result.attentionCount).toBe(2);
    expect(
      result.items.find((item) => item.titleKey.includes("notChecked"))?.status,
    ).toBe("info");
  });

  it("reports MCP as truthful inventory and never as synthetic availability", () => {
    const result = aggregateQuickCheck(
      [tool("claude-code")],
      snapshot({
        providers: [
          {
            tool: "claude-code",
            configured: true,
            configuredCount: 1,
            checkTargets: [],
          },
        ],
        configs: [{ tool: "claude-code", status: "readable" }],
        mcp: { total: 8, enabled: 2 },
      }),
      {},
    );
    const mcp = result.items.find((item) => item.kind === "mcp");
    expect(mcp).toMatchObject({ status: "info" });
    expect(mcp?.values).toEqual({ total: 8, enabled: 2 });
    expect(result.attentionCount).toBe(0);
    expect(result.status).toBe("ready");
    expect(result.configuredServiceCount).toBe(1);
    expect(result.mcpTotalCount).toBe(8);
    expect(result.mcpEnabledCount).toBe(2);
  });

  it("does not treat already-usable local MCP connections as a setup problem", () => {
    const local: LocalExtensionInventory = {
      items: [
        {
          kind: "mcp",
          id: "browser",
          scope: { kind: "tool", id: "claude-code" },
          name: "Browser",
          description: null,
          management: "detected",
          enabled: true,
          canDisable: true,
        },
        {
          kind: "mcp",
          id: "files",
          scope: { kind: "tool", id: "claude-code" },
          name: "Files",
          description: null,
          management: "detected",
          enabled: true,
          canDisable: true,
        },
      ],
      scopes: [{ tool: "claude-code", kind: "mcp", status: "ready" }],
      truncated: false,
    };
    const result = aggregateQuickCheck(
      [tool("claude-code")],
      snapshot(),
      {},
      local,
    );

    expect(
      result.items.some(
        (item) => item.kind === "mcp" && item.toolId === "claude-code",
      ),
    ).toBe(false);
    expect(result.attentionCount).toBe(0);
  });

  it("never turns an unavailable MCP inventory into a false ready signal", () => {
    const result = aggregateQuickCheck(
      [tool("claude-code")],
      snapshot(),
      {},
      null,
    );

    expect(
      result.items.find((item) => item.resolution === "reviewExtensions"),
    ).toMatchObject({ kind: "mcp", status: "attention", toolId: null });
    expect(result.status).toBe("attention");
  });

  it("summarizes configured services once per installed tool", () => {
    const provider = {
      tool: "claude-code" as const,
      configured: true,
      configuredCount: 2,
      checkTargets: [],
    };
    const result = aggregateQuickCheck(
      [tool("claude-code"), tool("codex", "notInstalled")],
      snapshot({
        providers: [
          provider,
          provider,
          {
            tool: "codex",
            configured: true,
            configuredCount: 4,
            checkTargets: [],
          },
        ],
        mcp: { total: 5, enabled: 3 },
      }),
      {},
    );

    expect(result.configuredServiceCount).toBe(2);
    expect(result.mcpTotalCount).toBe(5);
    expect(result.mcpEnabledCount).toBe(3);
  });

  it("deduplicates repeated source rows before calculating attention", () => {
    const provider = {
      tool: "claude-code" as const,
      configured: false,
      configuredCount: 0,
      checkTargets: [{ providerId: "relay", name: "Relay" }],
    };
    const result = aggregateQuickCheck(
      [
        tool("claude-code", "updateAvailable"),
        tool("claude-code", "updateAvailable"),
      ],
      snapshot({
        providers: [provider, provider],
        configs: [
          { tool: "claude-code", status: "unreadable" },
          { tool: "claude-code", status: "unreadable" },
        ],
      }),
      {},
    );
    // The pending update and the unreadable config; the absent service is
    // informational and must not inflate the count.
    expect(result.attentionCount).toBe(2);
    expect(result.items.filter((item) => item.kind === "tool")).toHaveLength(1);
    expect(
      result.items.filter((item) => item.kind === "provider"),
    ).toHaveLength(1);
    expect(result.items.filter((item) => item.kind === "config")).toHaveLength(
      1,
    );
  });

  it("orders action-required signals before attention and informational signals", () => {
    const result = aggregateQuickCheck(
      [tool("claude-code", "updateAvailable"), tool("codex", "broken")],
      snapshot({
        providers: [
          {
            tool: "claude-code",
            configured: true,
            configuredCount: 1,
            checkTargets: [],
          },
        ],
        configs: [{ tool: "claude-code", status: "readable" }],
      }),
      {},
    );

    expect(result.items.map((item) => item.status)).toEqual([
      "action",
      "attention",
      "ready",
      "ready",
      "info",
    ]);
  });
});
