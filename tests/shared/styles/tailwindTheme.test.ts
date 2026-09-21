import { createRequire } from "node:module";
import { describe, expect, it } from "vitest";

const require = createRequire(import.meta.url);

interface ThemeExtend {
  colors: Record<string, string | Record<string, string>>;
  fontFamily: Record<string, string[]>;
  fontSize: Record<string, unknown>;
  borderRadius: Record<string, string>;
  boxShadow: Record<string, string>;
  transitionDuration: Record<string, string>;
  transitionTimingFunction: Record<string, string>;
  animation: Record<string, string>;
  keyframes: Record<string, unknown>;
}

const config = require("../../../tailwind.config.cjs") as {
  darkMode: unknown;
  theme: { extend: ThemeExtend };
};
const extend = config.theme.extend;

describe("tailwind theme", () => {
  it("keeps the class-based dark mode strategy", () => {
    expect(config.darkMode).toEqual(["selector", ".dark"]);
  });

  it("exposes the product colour vocabulary through the HSL bridge", () => {
    expect(extend.colors.canvas).toEqual({
      DEFAULT: "hsl(var(--ui-bg-primary))",
      subtle: "hsl(var(--ui-bg-secondary))",
    });
    expect(extend.colors.surface).toEqual({
      DEFAULT: "hsl(var(--ui-surface))",
      hover: "hsl(var(--ui-surface-hover))",
    });
    expect(extend.colors.content).toEqual({
      DEFAULT: "hsl(var(--ui-text-primary))",
      muted: "hsl(var(--ui-text-secondary))",
    });
    expect(extend.colors.brand).toEqual({
      DEFAULT: "hsl(var(--ui-accent))",
      hover: "hsl(var(--ui-accent-hover))",
      foreground: "hsl(var(--ui-accent-foreground))",
    });
    expect(extend.colors.line).toBe("hsl(var(--ui-border))");
    expect(extend.colors.success).toBe("hsl(var(--ui-success))");
    expect(extend.colors.warning).toBe("hsl(var(--ui-warning))");
    expect(extend.colors.danger).toBe("hsl(var(--ui-danger))");
  });

  it("never hands Tailwind a raw hex token (alpha modifiers would break)", () => {
    const flat = JSON.stringify(extend.colors);
    expect(flat).not.toContain("var(--accent)");
    expect(flat).not.toContain("var(--bg-primary)");
    expect(flat).not.toContain("var(--surface)");
    expect(flat).not.toContain("var(--text-primary)");
  });

  it("keeps the legacy palettes App.tsx depends on", () => {
    expect(extend.colors.blue).toBeDefined();
    expect(extend.colors.gray).toBeDefined();
  });

  it("drives type, radius, shadow and motion from tokens", () => {
    expect(extend.fontFamily.sans).toEqual(["var(--font-sans)"]);
    expect(extend.fontFamily.mono).toEqual(["var(--font-mono)"]);
    expect(extend.fontSize.display).toEqual([
      "28px",
      { lineHeight: "34px", letterSpacing: "-0.02em", fontWeight: "600" },
    ]);
    expect(extend.fontSize.body).toEqual([
      "14px",
      { lineHeight: "20px", fontWeight: "400" },
    ]);
    expect(extend.borderRadius).toMatchObject({
      sm: "var(--radius-sm)",
      md: "var(--radius-md)",
      lg: "var(--radius-lg)",
      xl: "var(--radius-xl)",
      "2xl": "var(--radius-2xl)",
    });
    expect(extend.boxShadow).toEqual({
      sm: "var(--shadow-sm)",
      md: "var(--shadow-md)",
      lg: "var(--shadow-lg)",
    });
    expect(extend.transitionDuration).toMatchObject({
      fast: "var(--motion-fast)",
      base: "var(--motion-base)",
      modal: "var(--motion-modal)",
      page: "var(--motion-page)",
    });
    expect(extend.transitionTimingFunction).toMatchObject({
      standard: "var(--ease-standard)",
      page: "var(--ease-page)",
    });
  });

  it("uses the dedicated page motion for route transitions", () => {
    expect(extend.animation["page-in"]).toBe(
      "page-in var(--motion-page) var(--ease-page) both",
    );
    expect(extend.keyframes["page-in"]).toEqual({
      from: {
        opacity: "0",
        transform: "translate3d(0, var(--page-enter-y, 44px), 0)",
      },
      to: { opacity: "1", transform: "translate3d(0, 0, 0)" },
    });
  });

  it("times the modal animations with the motion tokens", () => {
    expect(extend.animation["ds-overlay-in"]).toContain("var(--motion-modal)");
    expect(extend.animation["ds-modal-in"]).toContain("var(--motion-modal)");
    expect(extend.animation["ds-dialog-in"]).toContain("var(--motion-modal)");
    expect(extend.keyframes["ds-modal-in"]).toBeDefined();
    expect(extend.keyframes["ds-dialog-in"]).toEqual({
      from: {
        opacity: "0",
        transform: "translate(-50%, calc(-50% + 8px)) scale(0.98)",
      },
      to: {
        opacity: "1",
        transform: "translate(-50%, -50%) scale(1)",
      },
    });
  });
});
