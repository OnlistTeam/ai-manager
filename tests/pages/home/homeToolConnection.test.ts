import { describe, expect, it } from "vitest";
import type { Provider } from "@/entities/provider";
import type { Tool } from "@/entities/tool";
import type { QuickCheckItem } from "@/features/health";
import {
  connectableTools,
  pickerProviders,
  pinnedModel,
  shownElsewhereOnHome,
  toolConnection,
} from "@/pages/home/homeToolConnection";

const CAPABILITIES = {
  canInstall: true,
  canUpdate: true,
  canUninstall: true,
  canRepair: false,
  canManageProvider: true,
  canManageMcp: true,
  canManageSkills: true,
  canManagePrompts: true,
  canManageVersion: true,
  canLaunch: true,
};

function tool(overrides: Partial<Tool> = {}): Tool {
  return {
    id: "claude-code",
    name: "Claude Code",
    descriptionKey: "tool.claude-code.description",
    status: "installed",
    version: "1.0.0",
    latestVersion: null,
    capabilities: CAPABILITIES,
    sessionsInsideSettings: false,
    environment: null,
    ...overrides,
  };
}

function provider(overrides: Partial<Provider> = {}): Provider {
  return {
    id: "relay",
    tool: "claude-code",
    name: "Relay",
    kind: "custom",
    active: false,
    baseUrl: null,
    apiKey: null,
    websiteUrl: null,
    testable: true,
    canRemove: true,
    ...overrides,
  };
}

function item(overrides: Partial<QuickCheckItem>): QuickCheckItem {
  return {
    id: "item",
    toolId: "claude-code",
    kind: "tool",
    status: "attention",
    titleKey: "title",
    descriptionKey: "description",
    values: {},
    ...overrides,
  };
}

describe("connectableTools", () => {
  it("keeps installed tools that can use an endpoint, in inventory order", () => {
    const tools = [
      tool({ id: "codex", name: "Codex", status: "updateAvailable" }),
      tool({ id: "opencode", status: "notInstalled" }),
      tool({ id: "gemini-cli", status: "broken" }),
      tool({
        id: "kimi-code",
        capabilities: { ...CAPABILITIES, canManageProvider: false },
      }),
      tool(),
    ];

    expect(connectableTools(tools).map((entry) => entry.id)).toEqual([
      "codex",
      "claude-code",
    ]);
  });
});

describe("toolConnection", () => {
  it("waits for the list, and says so when it cannot be read", () => {
    expect(toolConnection(tool(), undefined, false)).toEqual({
      kind: "loading",
    });
    expect(toolConnection(tool(), undefined, true)).toEqual({
      kind: "unavailable",
    });
  });

  it("names the selected custom entry, and calls an official one a sign-in", () => {
    const relay = provider({ active: true });
    expect(toolConnection(tool(), [relay], false)).toEqual({
      kind: "service",
      provider: relay,
    });
    const official = provider({ id: "o", kind: "official", active: true });
    expect(toolConnection(tool(), [official, provider()], false)).toEqual({
      kind: "official",
      provider: official,
    });
  });

  it("never guesses that the only saved entry is in use", () => {
    expect(toolConnection(tool(), [provider()], false)).toEqual({
      kind: "notConnected",
    });
  });

  it("falls back to the tool's own sign-in only when the tool has one", () => {
    const vendor = tool({
      discovery: {
        publisher: "Anthropic",
        access: "vendorOrProvider",
        useCases: ["officialCoding"],
      },
    });
    const endpointOnly = tool({
      discovery: {
        publisher: "SST",
        access: "provider",
        useCases: ["modelChoice"],
      },
    });
    expect(toolConnection(vendor, [], false)).toEqual({
      kind: "official",
      provider: null,
    });
    expect(toolConnection(endpointOnly, [], false)).toEqual({
      kind: "notConnected",
    });
  });

  it("counts additive entries instead of naming one", () => {
    expect(
      toolConnection(
        tool(),
        [provider({ additive: true }), provider({ id: "b", additive: true })],
        false,
      ),
    ).toEqual({ kind: "added", count: 2 });
  });
});

describe("pickerProviders", () => {
  it("lists every non-additive entry, the one in use first, the rest in saved order", () => {
    const choices = pickerProviders([
      provider({ id: "b" }),
      provider({ id: "a", active: true }),
      provider({ id: "c", additive: true }),
      provider({ id: "d" }),
    ]);
    expect(choices.map((entry) => entry.id)).toEqual(["a", "b", "d"]);
    expect(pickerProviders(undefined)).toEqual([]);
  });
});

describe("pinnedModel", () => {
  it("shows a model only when exactly one is pinned", () => {
    const profile = {
      providerId: "relay",
      baseUrl: null,
      endpointCandidates: [],
      endpointAutoSelect: false,
      models: ["kimi-k2"],
      headerNames: [],
      capabilities: {
        canEditBaseUrl: true,
        canEditEndpoints: true,
        canEditModels: true,
        canEditHeaders: false,
        supportsMultipleModels: false,
      },
      baseUrlTakesNoVersion: false,
    };
    expect(pinnedModel(profile)).toBe("kimi-k2");
    expect(pinnedModel({ ...profile, models: ["a", "b"] })).toBeNull();
    expect(pinnedModel({ ...profile, models: [] })).toBeNull();
    expect(pinnedModel(undefined)).toBeNull();
  });
});

describe("shownElsewhereOnHome", () => {
  const rows = new Set(["claude-code"]);

  it("leaves missing endpoints to the rows and updates to the status line", () => {
    expect(shownElsewhereOnHome(item({ resolution: "updateTool" }), rows)).toBe(
      true,
    );
    expect(
      shownElsewhereOnHome(
        item({ kind: "provider", resolution: "connectService" }),
        rows,
      ),
    ).toBe(true);
  });

  it("keeps every other finding, and every finding of an unlisted tool", () => {
    expect(
      shownElsewhereOnHome(
        item({ kind: "config", resolution: "reviewService" }),
        rows,
      ),
    ).toBe(false);
    expect(
      shownElsewhereOnHome(
        item({ toolId: "kimi-code", resolution: "updateTool" }),
        rows,
      ),
    ).toBe(false);
    expect(
      shownElsewhereOnHome(
        item({ toolId: null, kind: "mcp", resolution: "reviewMcp" }),
        rows,
      ),
    ).toBe(false);
  });
});
