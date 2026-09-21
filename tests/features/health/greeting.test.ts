import { describe, expect, it } from "vitest";
import { greetingSlot } from "@/features/health";

describe("greetingSlot", () => {
  it("splits the day the way spec section 27 greets the user", () => {
    expect(greetingSlot(5)).toBe("morning");
    expect(greetingSlot(11)).toBe("morning");
    expect(greetingSlot(12)).toBe("afternoon");
    expect(greetingSlot(17)).toBe("afternoon");
    expect(greetingSlot(18)).toBe("evening");
    expect(greetingSlot(23)).toBe("evening");
  });

  it("greets the small hours as evening rather than morning", () => {
    expect(greetingSlot(0)).toBe("evening");
    expect(greetingSlot(4)).toBe("evening");
  });
});
