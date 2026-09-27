import { describe, expect, it } from "vitest";
import type { EffectiveConnection, Provider } from "@/entities/provider";
import type { Tool } from "@/entities/tool";
import type { QuickCheckItem } from "@/features/health";
import {
  connectableTools,
  inUseProviderId,
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

function effective(
  overrides: Partial<EffectiveConnection> = {},
): EffectiveConnection {
  return {
    selection: "configuration",
    endpoint: "https://api.relay.example/",
    endpointSource: {
      kind: "shellFile",
      variable: "ANTHROPIC_BASE_URL",
      path: "~/.config/zsh/relay.zsh",
    },
    credential: "configured",
    credentialSource: {
      kind: "shellFile",
      variable: "ANTHROPIC_AUTH_TOKEN",
      path: "~/.config/zsh/relay.zsh",
    },
    providerId: null,
    shellInspected: true,
    outranksSwitch: false,
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

describe("toolConnection with the effective connection", () => {
  const official = provider({
    id: "official",
    name: "Claude Official",
    kind: "official",
    active: true,
  });

  it("names an address set outside this app over the saved selection", () => {
    const connection = effective();
    expect(toolConnection(tool(), [official], false, connection)).toEqual({
      kind: "external",
      connection,
    });
  });

  it("names the saved endpoint the tool really uses, even when another is selected", () => {
    const relay = provider();
    expect(
      toolConnection(
        tool(),
        [official, relay],
        false,
        effective({ providerId: "relay" }),
      ),
    ).toEqual({ kind: "service", provider: relay });
  });

  it("keeps the saved answer when the evidence says nothing definite", () => {
    const saved = { kind: "official", provider: official };
    expect(toolConnection(tool(), [official], false, null)).toEqual(saved);
    expect(
      toolConnection(
        tool(),
        [official],
        false,
        effective({ selection: "unknown" }),
      ),
    ).toEqual(saved);
    // The tool on its own default is what the saved list already says.
    expect(
      toolConnection(
        tool(),
        [official],
        false,
        effective({ endpoint: null, endpointSource: { kind: "toolDefault" } }),
      ),
    ).toEqual(saved);
    // A match the list no longer has is not invented.
    expect(
      toolConnection(
        tool(),
        [official],
        false,
        effective({ providerId: "gone" }),
      ),
    ).toEqual(saved);
  });

  it("still counts additive entries", () => {
    expect(
      toolConnection(
        tool(),
        [provider({ additive: true })],
        false,
        effective(),
      ),
    ).toEqual({ kind: "added", count: 1 });
  });
});

describe("pickerProviders", () => {
  it("lists every non-additive entry, the one in use first, the rest in saved order", () => {
    const choices = pickerProviders(
      [
        provider({ id: "b" }),
        provider({ id: "a", active: true }),
        provider({ id: "c", additive: true }),
        provider({ id: "d" }),
      ],
      "a",
    );
    expect(choices.map((entry) => entry.id)).toEqual(["a", "b", "d"]);
    expect(pickerProviders(undefined, null)).toEqual([]);
  });

  it("keeps the saved order when none of them is in use", () => {
    const choices = pickerProviders(
      [provider({ id: "b" }), provider({ id: "a", active: true })],
      null,
    );
    expect(choices.map((entry) => entry.id)).toEqual(["b", "a"]);
  });
});

describe("inUseProviderId", () => {
  it("is the saved entry the row names, and nothing for an outside address", () => {
    expect(
      inUseProviderId({ kind: "service", provider: provider({ id: "x" }) }),
    ).toBe("x");
    expect(inUseProviderId({ kind: "official", provider: null })).toBeNull();
    expect(
      inUseProviderId({
        kind: "external",
        connection: { ...effective(), endpoint: "https://a.example/" },
      }),
    ).toBeNull();
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
