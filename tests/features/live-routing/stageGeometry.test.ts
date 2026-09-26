import { describe, expect, it } from "vitest";
import {
  curveBetween,
  easeInOut,
  stageWires,
} from "@/features/live-routing/stageGeometry";

describe("stage geometry", () => {
  it("draws a horizontal S-curve bending halfway between the ends", () => {
    expect(curveBetween({ x: 10, y: 20 }, { x: 110, y: 60 })).toBe(
      "M10 20 C60 20 60 60 110 60",
    );
    expect(curveBetween({ x: 0.04, y: 1.26 }, { x: 3.33, y: 1.26 })).toBe(
      "M0 1.3 C1.7 1.3 1.7 1.3 3.3 1.3",
    );
  });

  it("runs tool wires into the hub's left edge and service wires out of its right", () => {
    const wires = stageWires({
      hub: { left: 200, top: 40, right: 280, bottom: 100 },
      sources: [
        {
          wire: "tool:codex",
          box: { left: 0, top: 0, right: 120, bottom: 32 },
        },
      ],
      targets: [
        {
          wire: "endpoint:a",
          box: { left: 360, top: 90, right: 560, bottom: 122 },
        },
      ],
    });
    expect(wires).toEqual([
      { id: "tool:codex", d: "M120 16 C160 16 160 70 200 70" },
      { id: "endpoint:a", d: "M280 70 C320 70 320 106 360 106" },
    ]);
  });

  it("eases in and out and clamps outside the leg", () => {
    expect(easeInOut(-1)).toBe(0);
    expect(easeInOut(0)).toBe(0);
    expect(easeInOut(0.5)).toBeCloseTo(0.5);
    expect(easeInOut(0.1)).toBeLessThan(0.1);
    expect(easeInOut(1)).toBe(1);
    expect(easeInOut(3)).toBe(1);
  });
});
