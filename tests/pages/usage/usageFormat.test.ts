import { describe, expect, it } from "vitest";
import { formatUsd } from "@/pages/usage/usageFormat";

describe("formatUsd", () => {
  it("presents estimated USD totals at normal currency precision", () => {
    expect(formatUsd("5820.701088", "en-US")).toBe("$5,820.70");
    expect(formatUsd("not-a-number", "en-US")).toBe("$0.00");
  });
});
