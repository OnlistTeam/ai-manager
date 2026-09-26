import { render } from "@testing-library/react";
import { LoaderCircle } from "lucide-react";
import { describe, expect, it } from "vitest";
import { SpatialScene } from "@/shared/ui/SpatialScene";

const TERMINAL = { sheet: "/terminal.webp", frames: 25, cols: 5, rows: 5 };

function turntableOf(container: HTMLElement): HTMLElement {
  const node = container.querySelector<HTMLElement>(
    ".spatial-scene__turntable",
  );
  if (!node) throw new Error("the scene rendered no turntable");
  return node;
}

describe("SpatialScene turntable", () => {
  /**
   * Without this starting pose, the element falls back to CSS's default 0%,
   * i.e. the first frame of the sequence — one extreme of yaw. Any caller
   * that hasn't wired up the pointer yet would end up with the model twisted
   * off to one side.
   */
  it("starts on the neutral frame rather than the first one", () => {
    const { container } = render(
      <SpatialScene
        icon={LoaderCircle}
        model="environment"
        turntable={TERMINAL}
      />,
    );
    const style = turntableOf(container).style;
    expect(style.getPropertyValue("--turntable-neutral-x")).toBe("-40%");
    expect(style.getPropertyValue("--turntable-neutral-y")).toBe("-40%");
    expect(style.getPropertyValue("--turntable-cols")).toBe("5");
    expect(style.getPropertyValue("--turntable-rows")).toBe("5");
  });
});
