import { describe, expect, it } from "vitest";
import { cn } from "@/shared/ui/cn";

describe("cn", () => {
  it("keeps a product font-size utility and a text-color utility together", () => {
    expect(cn("text-body", "text-brand-foreground")).toBe(
      "text-body text-brand-foreground",
    );
  });

  it("dedupes two product font-size utilities, keeping the later one", () => {
    expect(cn("text-body", "text-caption")).toBe("text-caption");
  });

  it("dedupes conflicting background-color utilities, keeping the later one", () => {
    expect(cn("bg-brand", "bg-danger")).toBe("bg-danger");
  });

  it("dedupes conflicting transition-duration utilities, keeping the later one", () => {
    expect(cn("duration-fast", "duration-200")).toBe("duration-200");
  });

  it("dedupes conflicting transition-timing-function utilities, keeping the later one", () => {
    expect(cn("ease-standard", "ease-linear")).toBe("ease-linear");
  });

  it("keeps a product motion utility and an unrelated utility together", () => {
    expect(cn("duration-fast", "text-content")).toBe(
      "duration-fast text-content",
    );
  });
});
