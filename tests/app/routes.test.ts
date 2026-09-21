import { describe, expect, it } from "vitest";
import { APP_ROUTES, DEFAULT_ROUTE, NAV_ITEMS, isAppRoute } from "@/app/routes";

describe("app routes", () => {
  it("keeps eight destinations in rail order", () => {
    expect(APP_ROUTES).toEqual([
      "home",
      "tools",
      "services",
      "extensions",
      "mcp",
      "prompts",
      "data",
      "settings",
    ]);
    expect(NAV_ITEMS.map((item) => item.id)).toEqual([...APP_ROUTES]);
    expect(DEFAULT_ROUTE).toBe("home");
  });

  it("pins only Settings to the rail footer", () => {
    expect(
      NAV_ITEMS.filter((item) => item.placement === "footer").map(
        (item) => item.id,
      ),
    ).toEqual(["settings"]);
    expect(NAV_ITEMS.find((item) => item.id === "settings")).toMatchObject({
      labelKey: "nav.settings",
      placement: "footer",
    });
  });

  it("folds the former power-tool pages into their parent pages", () => {
    for (const merged of ["routing", "usage", "sessions", "workspace"]) {
      expect(isAppRoute(merged)).toBe(false);
    }
    expect(NAV_ITEMS.every((item) => !("advancedOnly" in item))).toBe(true);
  });

  it("gives every route one nav entry, in navigation order", () => {
    expect(new Set(NAV_ITEMS.map((item) => item.labelKey)).size).toBe(
      NAV_ITEMS.length,
    );
    expect(
      NAV_ITEMS.every(
        (item) =>
          typeof item.icon === "function" || typeof item.icon === "object",
      ),
    ).toBe(true);
  });

  it("does not expose the jargon entries spec section 24 bans by default", () => {
    for (const banned of [
      "provider",
      "proxy",
      "failover",
      "endpoint",
      "session",
      "environment",
    ]) {
      expect(APP_ROUTES).not.toContain(banned);
    }
  });

  it("narrows unknown values", () => {
    expect(isAppRoute("tools")).toBe(true);
    expect(isAppRoute("nope")).toBe(false);
    expect(isAppRoute(null)).toBe(false);
    expect(isAppRoute(3)).toBe(false);
  });
});
