import { render } from "@testing-library/react";
import { LoaderCircle } from "lucide-react";
import { describe, expect, it } from "vitest";
import { SpatialScene } from "@/shared/ui/SpatialScene";
import { EnvironmentHeroSkeleton } from "@/pages/home/EnvironmentHeroSkeleton";
import { HomeUnavailableState } from "@/pages/home/HomeReadinessNotice";

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

/**
 * Whenever a turntable is rendered on a stage, the pointer must be given the
 * same layout too. Without it, yaw falls back to "CSS-rotating a flat
 * picture," which, layered on top of the turntable's own frame, rotates the
 * same object twice — and the model would ignore the mouse for the whole
 * loading period. A stage writing the neutral frame onto itself is exactly
 * the evidence that it's wired up.
 */
describe("every stage that renders a turntable also drives it", () => {
  it.each([
    [
      "loading skeleton",
      <EnvironmentHeroSkeleton key="s" title="Environment" label="Checking" />,
    ],
    [
      "unavailable state",
      <HomeUnavailableState
        key="u"
        retrying={false}
        retryButtonRef={{ current: null }}
        onRetry={() => {}}
      />,
    ],
  ])("%s", (_name, element) => {
    const { container } = render(element);
    const stage = container.querySelector<HTMLElement>("[data-spatial-stage]");
    expect(stage).not.toBeNull();
    expect(stage?.style.getPropertyValue("--spatial-frame-x")).toBe("-40%");
    expect(stage?.style.getPropertyValue("--spatial-frame-y")).toBe("-40%");
    // While a turntable is present, yaw is carried by the frame — CSS must
    // not rotate the sprite again.
    expect(stage?.style.getPropertyValue("--spatial-rotate-y")).toBe("0deg");
  });
});
