import { describe, expect, it } from "vitest";
import type { EffectiveConnection, Provider } from "@/entities/provider";
import {
  overrideSourceFor,
  providerEffectiveState,
  sortByEffectiveState,
} from "@/pages/services/providerEffectiveState";

function provider(id: string, active: boolean): Provider {
  return {
    id,
    tool: "claude-code",
    name: id,
    kind: "custom",
    active,
    baseUrl: null,
    apiKey: null,
    websiteUrl: null,
    testable: false,
    canRemove: !active,
  };
}

const shellOverride: EffectiveConnection = {
  endpoint: "https://api.example.test/",
  endpointSource: { kind: "environment", variable: "ANTHROPIC_BASE_URL" },
  credential: "configured",
  credentialSource: { kind: "environment", variable: "ANTHROPIC_AUTH_TOKEN" },
  providerId: null,
  shellInspected: true,
};

describe("providerEffectiveState", () => {
  it("marks the matching record as in use and the selected one as overridden", () => {
    const matched: EffectiveConnection = {
      ...shellOverride,
      providerId: "relay",
    };
    expect(providerEffectiveState(provider("relay", false), matched)).toBe(
      "inUse",
    );
    expect(providerEffectiveState(provider("official", true), matched)).toBe(
      "overridden",
    );
    expect(providerEffectiveState(provider("other", false), matched)).toBe(
      "saved",
    );
  });

  it("never treats a database selection as proof when runtime evidence is absent", () => {
    expect(providerEffectiveState(provider("official", true), undefined)).toBe(
      "unknown",
    );
    expect(providerEffectiveState(provider("official", true), null)).toBe(
      "unknown",
    );
    expect(providerEffectiveState(provider("other", false), undefined)).toBe(
      "unknown",
    );
  });

  it.each(["recentModel", "defaultModel"] as const)(
    "keeps %s distinct from in-use",
    (selection) => {
      const connection = { ...shellOverride, providerId: "relay", selection };
      expect(providerEffectiveState(provider("relay", false), connection)).toBe(
        selection,
      );
      expect(
        sortByEffectiveState(
          [provider("other", false), provider("relay", false)],
          connection,
        )[0]?.id,
      ).toBe("relay");
    },
  );

  it("ignores stale matches when resolution failed", () => {
    expect(
      providerEffectiveState(provider("relay", true), {
        ...shellOverride,
        selection: "unknown",
        providerId: "relay",
      }),
    ).toBe("unknown");
  });

  it("names the variable or file that overrides the selection", () => {
    expect(overrideSourceFor(shellOverride)).toBe("ANTHROPIC_BASE_URL");
    expect(
      overrideSourceFor({
        ...shellOverride,
        endpointSource: { kind: "toolDefault" },
        credentialSource: { kind: "liveConfig", path: "~/.codex/auth.json" },
      }),
    ).toBe("~/.codex/auth.json");
    expect(
      overrideSourceFor({
        ...shellOverride,
        endpointSource: { kind: "toolDefault" },
        credentialSource: { kind: "toolDefault" },
      }),
    ).toBeNull();
  });

  it("orders in-use first, then overridden, then the rest", () => {
    const matched: EffectiveConnection = {
      ...shellOverride,
      providerId: "relay",
    };
    const sorted = sortByEffectiveState(
      [
        provider("a", false),
        provider("official", true),
        provider("relay", false),
      ],
      matched,
    );
    expect(sorted.map((item) => item.id)).toEqual(["relay", "official", "a"]);
  });
});
