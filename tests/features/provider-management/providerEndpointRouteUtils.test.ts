import { describe, expect, it } from "vitest";
import { trailingVersionSegment } from "@/features/provider-management/providerEndpointRouteUtils";

describe("trailingVersionSegment", () => {
  it("names the version segment an address ends in", () => {
    expect(trailingVersionSegment("https://api.example.test/v1")).toBe("v1");
    expect(trailingVersionSegment("https://api.example.test/v1/")).toBe("v1");
    expect(trailingVersionSegment("https://api.example.test/v1beta")).toBe(
      "v1beta",
    );
    expect(trailingVersionSegment("https://api.example.test/v10")).toBe("v10");
  });

  it("ignores addresses whose last segment is not a version", () => {
    expect(trailingVersionSegment("https://api.example.test")).toBeNull();
    expect(
      trailingVersionSegment("https://api.example.test/openai"),
    ).toBeNull();
    expect(
      trailingVersionSegment("https://api.example.test/v1/messages"),
    ).toBeNull();
    expect(trailingVersionSegment("https://v1.example.test")).toBeNull();
    expect(trailingVersionSegment("not a url")).toBeNull();
  });
});
