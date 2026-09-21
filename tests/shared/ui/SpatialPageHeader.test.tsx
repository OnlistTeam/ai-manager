import fs from "node:fs";
import path from "node:path";
import { fireEvent, render, screen } from "@testing-library/react";
import { Boxes } from "lucide-react";
import { afterEach, describe, expect, it } from "vitest";
import { SpatialPageHeader } from "@/shared/ui/SpatialPageHeader";
import { SpatialScene } from "@/shared/ui/SpatialScene";

const UI_ROOT = path.resolve(
  __dirname,
  "..",
  "..",
  "..",
  "src",
  "shared",
  "ui",
);
const CSS_PATH = path.resolve(__dirname, "..", "..", "..", "src", "index.css");
const originalAnimationFrame = window.requestAnimationFrame;
const originalMatchMedia = window.matchMedia;
const originalVisibilityState = document.visibilityState;

afterEach(() => {
  Object.defineProperty(window, "requestAnimationFrame", {
    configurable: true,
    value: originalAnimationFrame,
  });
  Object.defineProperty(window, "matchMedia", {
    configurable: true,
    value: originalMatchMedia,
  });
  Object.defineProperty(document, "visibilityState", {
    configurable: true,
    value: originalVisibilityState,
  });
});

/**
 * Centres the artwork at (0, 0) so a cursor one REACH away lands on a whole
 * vector and the expected CSS values stay readable.
 */
function stubOrigin(stage: HTMLElement): void {
  const origin =
    stage.querySelector<HTMLElement>("[data-spatial-origin]") ?? stage;
  Object.defineProperty(origin, "getBoundingClientRect", {
    configurable: true,
    value: () => ({
      left: -50,
      top: -50,
      right: 50,
      bottom: 50,
      width: 100,
      height: 100,
      x: -50,
      y: -50,
      toJSON: () => ({}),
    }),
  });
}

/** One full reach right and up: vector (1, -1). */
function fullReachPointerMove(): Event {
  const event = new Event("pointermove", { bubbles: true });
  Object.defineProperties(event, {
    clientX: { value: 620 },
    clientY: { value: -620 },
    pointerType: { value: "mouse" },
  });
  return event;
}

describe("SpatialPageHeader", () => {
  it("keeps the page title, description and action semantic", () => {
    render(
      <SpatialPageHeader
        title="AI Tools"
        description="Install and update local tools."
        icon={Boxes}
        model="tools"
        action={<button type="button">Check again</button>}
      />,
    );

    expect(
      screen.getByRole("heading", { level: 1, name: "AI Tools" }),
    ).toBeInTheDocument();
    expect(
      screen.getByText("Install and update local tools."),
    ).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Check again" })).toBeEnabled();
  });

  it("writes bounded pointer direction to CSS and resets on leave", () => {
    Object.defineProperty(window, "requestAnimationFrame", {
      configurable: true,
      value: undefined,
    });
    const { container } = render(
      <SpatialPageHeader title="AI Tools" icon={Boxes} model="tools" />,
    );
    const stage = container.querySelector<HTMLElement>(".spatial-page-hero");
    expect(stage).not.toBeNull();
    stubOrigin(stage!);

    const pointerMove = fullReachPointerMove();
    window.dispatchEvent(pointerMove);
    expect(stage?.style.getPropertyValue("--spatial-rotate-y")).toBe("26deg");
    expect(stage?.style.getPropertyValue("--spatial-rotate-x")).toBe("12deg");
    expect(stage?.style.getPropertyValue("--spatial-detail-x")).toBe("6px");
    expect(stage?.style.getPropertyValue("--spatial-detail-y")).toBe("-9px");
    expect(stage?.style.getPropertyValue("--spatial-stage-glow-x")).toBe("35%");
    expect(stage?.style.getPropertyValue("--spatial-stage-glow-y")).toBe("24%");
    expect(stage?.style.getPropertyValue("--spatial-reflection-x")).toBe("50%");
    expect(stage?.style.getPropertyValue("--spatial-reflection-y")).toBe("4%");

    // The cursor moving off the card does not neutralise the pose — the model
    // keeps facing it anywhere in the window.
    fireEvent.pointerLeave(stage!);
    expect(stage?.style.getPropertyValue("--spatial-rotate-y")).toBe("26deg");

    const halfReach = new Event("pointermove", { bubbles: true });
    Object.defineProperties(halfReach, {
      clientX: { value: -620 },
      clientY: { value: 0 },
      pointerType: { value: "mouse" },
    });
    window.dispatchEvent(halfReach);
    expect(stage?.style.getPropertyValue("--spatial-rotate-y")).toBe("-26deg");
  });

  it("does not track the pointer when reduced motion is requested", () => {
    Object.defineProperty(window, "requestAnimationFrame", {
      configurable: true,
      value: undefined,
    });
    Object.defineProperty(window, "matchMedia", {
      configurable: true,
      value: (query: string) =>
        ({
          matches: query === "(prefers-reduced-motion: reduce)",
          media: query,
          onchange: null,
          addListener: () => {},
          removeListener: () => {},
          addEventListener: () => {},
          removeEventListener: () => {},
          dispatchEvent: () => false,
        }) as MediaQueryList,
    });
    const { container } = render(
      <SpatialPageHeader title="AI Tools" icon={Boxes} model="tools" />,
    );
    const stage = container.querySelector<HTMLElement>(".spatial-page-hero");
    expect(stage).not.toBeNull();
    stubOrigin(stage!);

    const pointerMove = fullReachPointerMove();
    window.dispatchEvent(pointerMove);

    expect(stage?.style.getPropertyValue("--spatial-rotate-y")).toBe("0deg");
    expect(stage?.style.getPropertyValue("--spatial-rotate-x")).toBe("0deg");
  });

  it("returns to a neutral pose when the desktop window blurs or hides", () => {
    Object.defineProperty(window, "requestAnimationFrame", {
      configurable: true,
      value: undefined,
    });
    const { container } = render(
      <SpatialPageHeader title="AI Tools" icon={Boxes} model="tools" />,
    );
    const stage = container.querySelector<HTMLElement>(".spatial-page-hero");
    expect(stage).not.toBeNull();
    stubOrigin(stage!);

    const pointerMove = fullReachPointerMove();
    window.dispatchEvent(pointerMove);
    expect(stage?.style.getPropertyValue("--spatial-rotate-y")).toBe("26deg");

    window.dispatchEvent(new Event("blur"));
    expect(stage?.style.getPropertyValue("--spatial-rotate-y")).toBe("0deg");
    expect(stage?.style.getPropertyValue("--spatial-shift-x")).toBe("0px");

    window.dispatchEvent(pointerMove);
    expect(stage?.style.getPropertyValue("--spatial-rotate-y")).toBe("26deg");
    Object.defineProperty(document, "visibilityState", {
      configurable: true,
      value: "hidden",
    });
    document.dispatchEvent(new Event("visibilitychange"));

    expect(stage?.style.getPropertyValue("--spatial-rotate-y")).toBe("0deg");
    expect(stage?.style.getPropertyValue("--spatial-rotate-x")).toBe("0deg");
  });

  it("keeps scene geometry decorative and exposes model and tone hooks", () => {
    const { container } = render(
      <SpatialScene
        icon={Boxes}
        model="extensions"
        tone="warning"
        size="hero"
      />,
    );
    const scene = container.querySelector(".spatial-scene");
    expect(scene).toHaveAttribute("aria-hidden", "true");
    expect(scene).toHaveAttribute("data-model", "extensions");
    expect(scene).toHaveAttribute("data-tone", "warning");
    expect(scene).toHaveAttribute("data-size", "hero");
    expect(scene?.querySelectorAll(".spatial-scene__motif-part")).toHaveLength(
      4,
    );
    expect(
      scene?.querySelectorAll(".spatial-scene__sculpture-piece"),
    ).toHaveLength(4);
  });

  it("accepts a branded raster glyph without changing decorative semantics", () => {
    const { container } = render(
      <SpatialScene
        glyph={<img src="/brand.png" alt="" />}
        model="environment"
        loading
      />,
    );

    const scene = container.querySelector(".spatial-scene");
    expect(scene).toHaveAttribute("aria-hidden", "true");
    expect(scene).toHaveAttribute("data-loading", "true");
    expect(scene?.querySelector(".spatial-scene__glyph img")).toHaveAttribute(
      "src",
      "/brand.png",
    );
    expect(scene?.querySelector(".spatial-scene__glyph svg")).toBeNull();
  });

  it("places transparent route artwork on the same decorative tilt plane", () => {
    const { container } = render(
      <SpatialPageHeader
        title="AI Tools"
        icon={Boxes}
        model="tools"
        artwork="/spatial/tools.png"
      />,
    );

    const scene = container.querySelector(".spatial-scene");
    expect(scene).toHaveAttribute("aria-hidden", "true");
    expect(scene).toHaveAttribute("data-artwork", "true");
    expect(scene?.querySelector(".spatial-scene__artwork")).toHaveAttribute(
      "src",
      "/spatial/tools.png",
    );
    expect(scene?.querySelector(".spatial-scene__core")).toBeNull();
    expect(
      scene?.querySelector(".spatial-scene__artwork-glint"),
    ).not.toBeNull();
  });

  it("exposes the route model and tone on the complete visual stage", () => {
    const { container } = render(
      <SpatialPageHeader
        title="AI Services"
        icon={Boxes}
        model="services"
        tone="danger"
      />,
    );

    const stage = container.querySelector("[data-spatial-stage]");
    expect(stage).toHaveAttribute("data-model", "services");
    expect(stage).toHaveAttribute("data-tone", "danger");
  });

  it("forwards loading semantics and motion to the decorative model", () => {
    const { container } = render(
      <SpatialPageHeader title="AI Tools" icon={Boxes} model="tools" loading />,
    );

    const stage = container.querySelector("[data-spatial-stage]");

    expect(stage).toHaveAttribute("aria-busy", "true");
    expect(stage?.querySelector('[data-model="tools"]')).toHaveAttribute(
      "data-loading",
      "true",
    );
  });

  it("uses product tokens and includes a reduced-motion contract", () => {
    const sources = [
      fs.readFileSync(path.join(UI_ROOT, "SpatialPageHeader.tsx"), "utf8"),
      fs.readFileSync(path.join(UI_ROOT, "SpatialScene.tsx"), "utf8"),
      fs.readFileSync(CSS_PATH, "utf8"),
    ].join("\n");

    expect(sources).not.toMatch(/#[0-9a-fA-F]{3,8}\b/);
    expect(sources).toContain("@media (prefers-reduced-motion: reduce)");
    expect(sources).toContain("var(--ui-accent)");
    expect(sources).toContain("spatial-glyph-breathe");
    expect(sources).toContain("spatial-scene__core::before");
    expect(sources).toContain("spatial-scene__sculpture-piece");
    expect(sources).toContain("spatial-scene__artwork-shell");
    expect(sources).toContain("--spatial-detail-x");
    expect(sources).toContain("calc(0px - var(--spatial-shift-x))");
  });
});
