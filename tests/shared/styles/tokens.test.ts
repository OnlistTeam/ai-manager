import fs from "node:fs";
import path from "node:path";
import { describe, expect, it } from "vitest";

const TOKENS_CSS = path.resolve(
  __dirname,
  "..",
  "..",
  "..",
  "src",
  "shared",
  "styles",
  "tokens.css",
);
const INDEX_CSS = path.resolve(__dirname, "..", "..", "..", "src", "index.css");

/**
 * Every product colour is declared twice: once as hex (spec §46 names, the
 * source of truth) and once as HSL channels (`--ui-*`) so Tailwind's `/alpha`
 * modifiers keep working. This map is the contract between the two halves.
 */
const BRIDGE_TO_PRODUCT: Record<string, string> = {
  "--ui-bg-primary": "--bg-primary",
  "--ui-bg-secondary": "--bg-secondary",
  "--ui-surface": "--surface",
  "--ui-surface-hover": "--surface-hover",
  "--ui-text-primary": "--text-primary",
  "--ui-text-secondary": "--text-secondary",
  "--ui-border": "--border",
  "--ui-accent": "--accent",
  "--ui-accent-hover": "--accent-hover",
  "--ui-accent-foreground": "--accent-foreground",
  "--ui-success": "--success",
  "--ui-warning": "--warning",
  "--ui-danger": "--danger",
};

/** shadcn variable name -> the bridge variable it must alias. */
const SHADCN_ALIASES: Record<string, string> = {
  "--background": "--ui-bg-primary",
  "--foreground": "--ui-text-primary",
  "--card": "--ui-surface",
  "--card-foreground": "--ui-text-primary",
  "--popover": "--ui-surface",
  "--popover-foreground": "--ui-text-primary",
  "--primary": "--ui-accent",
  "--primary-foreground": "--ui-accent-foreground",
  "--secondary": "--ui-bg-secondary",
  "--secondary-foreground": "--ui-text-primary",
  "--muted": "--ui-bg-secondary",
  "--muted-foreground": "--ui-text-secondary",
  "--destructive": "--ui-danger",
  "--destructive-foreground": "--ui-accent-foreground",
  "--input": "--ui-border",
  "--ring": "--ui-accent",
};

const SCALE_TOKENS = [
  "--radius-sm",
  "--radius-md",
  "--radius-lg",
  "--radius-xl",
  "--radius-2xl",
  "--shadow-sm",
  "--shadow-md",
  "--shadow-lg",
  "--motion-fast",
  "--motion-base",
  "--motion-modal",
  "--motion-page",
  "--ease-standard",
  "--ease-page",
  "--font-sans",
  "--font-mono",
];

const source = fs.readFileSync(TOKENS_CSS, "utf8");

/** Reads the first top-level block for `selector` into a declaration map. */
function readBlock(selector: string): Record<string, string> {
  const match = source.match(new RegExp(`${selector}\\s*\\{([^}]*)\\}`));
  if (!match) throw new Error(`tokens.css is missing a ${selector} block`);
  const declarations: Record<string, string> = {};
  const pattern = /--([\w-]+):\s*([^;{}]+);/g;
  let declaration = pattern.exec(match[1]);
  while (declaration !== null) {
    declarations[`--${declaration[1]}`] = declaration[2]
      .replace(/\s+/g, " ")
      .trim();
    declaration = pattern.exec(match[1]);
  }
  return declarations;
}

/** `"161.6 39.7% 30.6%"` -> `"#2f6d5a"`. Mirrors the CSS `hsl()` function. */
function hslTripletToHex(triplet: string): string {
  const parts = triplet.split(/\s+/);
  if (parts.length !== 3) throw new Error(`not an HSL triplet: ${triplet}`);
  const hue = Number.parseFloat(parts[0]);
  const saturation = Number.parseFloat(parts[1]) / 100;
  const lightness = Number.parseFloat(parts[2]) / 100;
  const chroma = (1 - Math.abs(2 * lightness - 1)) * saturation;
  const sector = hue / 60;
  const second = chroma * (1 - Math.abs((sector % 2) - 1));
  const rgb =
    sector < 1
      ? [chroma, second, 0]
      : sector < 2
        ? [second, chroma, 0]
        : sector < 3
          ? [0, chroma, second]
          : sector < 4
            ? [0, second, chroma]
            : sector < 5
              ? [second, 0, chroma]
              : [chroma, 0, second];
  const offset = lightness - chroma / 2;
  const channels = rgb.map((value) =>
    Math.round((value + offset) * 255)
      .toString(16)
      .padStart(2, "0"),
  );
  return `#${channels.join("")}`;
}

function relativeLuminance(hex: string): number {
  const channels = hex
    .slice(1)
    .match(/.{2}/g)
    ?.map((channel) => Number.parseInt(channel, 16) / 255)
    .map((channel) =>
      channel <= 0.04045 ? channel / 12.92 : ((channel + 0.055) / 1.055) ** 2.4,
    );
  if (!channels || channels.length !== 3) {
    throw new Error(`not a hex colour: ${hex}`);
  }
  return channels[0] * 0.2126 + channels[1] * 0.7152 + channels[2] * 0.0722;
}

function contrastRatio(foreground: string, background: string): number {
  const lighter = Math.max(
    relativeLuminance(foreground),
    relativeLuminance(background),
  );
  const darker = Math.min(
    relativeLuminance(foreground),
    relativeLuminance(background),
  );
  return (lighter + 0.05) / (darker + 0.05);
}

describe("design tokens", () => {
  const root = readBlock(":root");

  it("keeps controls stationary on hover", () => {
    const css = fs.readFileSync(INDEX_CSS, "utf8");
    expect(css).not.toMatch(
      /\.ds-button:not\(:disabled\):(hover|active)\s*\{[^}]*transform:/s,
    );
    expect(css).not.toMatch(
      /\.tools-scan-action[^{}]*:(hover|active)[^{]*\{[^}]*transform:/s,
    );
    expect(css).not.toMatch(
      /\.sidebar-nav-item:hover \.sidebar-nav-icon\s*\{[^}]*transform:/s,
    );
  });

  it("declares every spec section 46 product token as hex", () => {
    for (const productToken of Object.values(BRIDGE_TO_PRODUCT)) {
      expect(root[productToken]).toMatch(/^#[0-9a-f]{6}$/);
    }
  });

  it("keeps the Tailwind bridge byte-identical to the hex tokens", () => {
    for (const [bridge, product] of Object.entries(BRIDGE_TO_PRODUCT)) {
      expect(root[bridge], `${bridge} is missing`).toBeDefined();
      expect(hslTripletToHex(root[bridge])).toBe(root[product]);
    }
  });

  /**
   * The product has one coloured appearance, so a second theme block would be a
   * second source of truth for every colour. This is the guard that keeps a
   * `.dark`/`.light` pair from creeping back in.
   */
  it("declares exactly one appearance", () => {
    expect(source).not.toMatch(/^\.dark\s*\{/m);
    expect(source).not.toMatch(/^\.light\s*\{/m);
  });

  /**
   * Depth on the coloured canvas is made of translucent white film. A layer
   * that stopped being white — or picked up an opaque value — would go back to
   * building surfaces by darkening, which is the look this replaced.
   */
  it("builds every lift layer out of translucent white", () => {
    for (const layer of [
      "--layer-1",
      "--layer-2",
      "--layer-3",
      "--hairline",
      "--hairline-strong",
    ]) {
      expect(root[layer], `${layer} is missing`).toMatch(
        /^hsl\(0 0% 100% \/ 0\.\d+\)$/,
      );
    }
  });

  it("aliases every shadcn variable onto a bridge variable", () => {
    for (const [alias, bridge] of Object.entries(SHADCN_ALIASES)) {
      expect(root[alias]).toBe(`var(${bridge})`);
    }
  });

  it("keeps secondary text AA-readable on muted surfaces", () => {
    expect(
      contrastRatio(root["--text-secondary"], root["--bg-secondary"]),
    ).toBeGreaterThanOrEqual(4.5);
  });

  it("does not reuse the three product token names for shadcn semantics", () => {
    for (const name of ["--accent", "--accent-foreground", "--border"]) {
      expect(root[name]).toMatch(/^#[0-9a-f]{6}$/);
    }
  });

  it("declares the radius, shadow, motion and type scales", () => {
    for (const token of SCALE_TOKENS) {
      expect(root[token], `${token} is missing`).toBeTruthy();
    }
  });

  /**
   * A page change takes `--motion-page`; the two transition components drop the
   * departing layer on a JS timer. If the two drift apart the outgoing page is
   * either cut off mid-animation or left on screen after it finishes.
   */
  it("keeps the page motion token in step with the transition constant", () => {
    const motion = fs.readFileSync(
      path.resolve(
        __dirname,
        "..",
        "..",
        "..",
        "src",
        "app",
        "useRouteMotion.ts",
      ),
      "utf8",
    );
    const constant = motion.match(/ROUTE_TRANSITION_MS = (\d+)/)?.[1];
    expect(root["--motion-page"]).toBe(`${constant}ms`);
  });

  it("declares a signature glow per home status", () => {
    for (const token of ["--glow-ready", "--glow-attention", "--glow-action"]) {
      expect(root[token]).toContain("radial-gradient");
    }
  });

  it("keeps content surfaces visibly lifted from the page canvas", () => {
    expect(relativeLuminance(root["--surface"])).toBeGreaterThan(
      relativeLuminance(root["--bg-secondary"]),
    );
    expect(
      contrastRatio(root["--text-secondary"], root["--surface"]),
    ).toBeGreaterThanOrEqual(4.5);
  });

  it("zeroes the motion tokens under prefers-reduced-motion", () => {
    const media = source.match(
      /@media \(prefers-reduced-motion: reduce\)\s*\{([\s\S]*?)\n\}/,
    );
    expect(media).not.toBeNull();
    expect(media?.[1]).toContain("--motion-fast: 0ms");
    expect(media?.[1]).toContain("--motion-base: 0ms");
    expect(media?.[1]).toContain("--motion-modal: 0ms");
    expect(media?.[1]).toContain("--motion-page: 0ms");
  });

  it("leaves index.css free of colour variable definitions", () => {
    const indexCss = fs.readFileSync(INDEX_CSS, "utf8");
    expect(indexCss).not.toContain("--background:");
    expect(indexCss).not.toContain("--primary:");
    expect(indexCss).not.toContain("--border:");
    expect(indexCss).not.toContain("hsl(var(--border))");
  });

  it("keeps scrolling while hiding scrollbar chrome until direct interaction", () => {
    const indexCss = fs.readFileSync(INDEX_CSS, "utf8");
    expect(indexCss).toMatch(
      /\.scrollbar-hidden\s*\{[^}]*scrollbar-width:\s*none/s,
    );
    expect(indexCss).toMatch(/\.scrollbar-hidden\s*\{[^}]*overflow-x:\s*clip/s);
    expect(indexCss).toMatch(
      /\.scrollbar-hidden::-webkit-scrollbar\s*\{[^}]*display:\s*none\s*!important/s,
    );
    expect(indexCss).toMatch(
      /\.scrollbar-subtle\s*\{[^}]*scrollbar-width:\s*thin/s,
    );
    expect(indexCss).toMatch(
      /\.scrollbar-subtle::-webkit-scrollbar\s*\{[^}]*display:\s*block\s*!important/s,
    );
    expect(indexCss).toMatch(
      /\.scrollbar-subtle\s*\{[^}]*scrollbar-color:\s*transparent transparent/s,
    );
    expect(indexCss).toMatch(
      /\.scrollbar-subtle:hover,[\s\S]*?scrollbar-color:\s*hsl\(var\(--ui-text-secondary\) \/ 0\.3\) transparent/s,
    );
    expect(indexCss).toContain("height: 0");
  });

  it("pins the application root to one non-scrolling window viewport", () => {
    const indexCss = fs.readFileSync(INDEX_CSS, "utf8");
    for (const selector of ["html", "body", "#root"]) {
      const escaped = selector === "#root" ? "#root" : selector;
      expect(indexCss).toMatch(
        new RegExp(
          `${escaped}\\s*\\{[^}]*height:\\s*100%[^}]*overflow:\\s*hidden`,
          "s",
        ),
      );
    }
    expect(indexCss).toMatch(/body\s*\{[^}]*position:\s*fixed[^}]*inset:\s*0/s);
    expect(indexCss).toMatch(
      /\.app-scroll-viewport,[\s\S]*?overflow-x:\s*hidden;[\s\S]*?overflow-x:\s*clip;/,
    );
    expect(indexCss).toMatch(
      /\.app-route-transition\[data-transition-direction="backward"\]\s*\{[^}]*--page-enter-y:\s*-88px[^}]*--page-exit-y:\s*52px/s,
    );
  });

  it("keeps the integrated title material soft while protecting focused content", () => {
    const indexCss = fs.readFileSync(INDEX_CSS, "utf8");
    expect(indexCss).toMatch(
      /\.app-scroll-viewport\s*\{[^}]*scroll-padding-top:\s*96px/s,
    );
    expect(indexCss).toMatch(
      /\.app-statusbar::before\s*\{[^}]*backdrop-filter:\s*blur\(22px\) saturate\(122%\)[^}]*mask-image:\s*linear-gradient/s,
    );
    expect(indexCss).toMatch(
      /\.app-sidebar\s*\{[^}]*user-select:\s*none[^}]*-webkit-user-select:\s*none/s,
    );
  });

  /*
   * The sidebar and the content area are two regions of the same canvas, not
   * two panels glued together. This test locks in the two conditions that
   * make them read as one, both of which are easy to break by accident in
   * later tweaks:
   *
   * 1. The top material belongs to the content area, but it must fade out on
   *    the edge that touches the sidebar. If it were cut off dead straight
   *    at x = sidebar width, a light/dark seam would appear at the top of
   *    the window — measured, the brightness difference between the two
   *    sides is 8.4/255, which is exactly where the "harshly divided" look
   *    comes from.
   * 2. The divider line must disappear completely at both the top and
   *    bottom of the window. A seam running the full height reads as a
   *    boundary between two surfaces; CleanMyMac's equivalent line is
   *    actually fairly bright in the middle (measured peak of 17.9/255), but
   *    it has already dropped to zero by 70% height — the sense of blending
   *    comes from the line vanishing at both ends, not from dimming it
   *    overall.
   */
  it("reads the rail and the page as one canvas, not two panels", () => {
    const indexCss = fs.readFileSync(INDEX_CSS, "utf8");

    const statusbar =
      indexCss.match(/\.app-statusbar\s*\{([^}]*)\}/)?.[1] ?? "";
    expect(statusbar).toContain("mask-image: linear-gradient(");
    expect(statusbar).toMatch(/90deg,\s*transparent 0%,\s*transparent 7%/);
    expect(statusbar).toContain("-webkit-mask-image:");

    const seam =
      indexCss.match(/\.app-sidebar::after\s*\{([^}]*)\}/)?.[1] ?? "";
    const top = Number(seam.match(/top:\s*([\d.]+)%/)?.[1]);
    const bottom = Number(seam.match(/bottom:\s*([\d.]+)%/)?.[1]);
    expect(top).toBeGreaterThanOrEqual(10);
    expect(bottom).toBeGreaterThanOrEqual(25);
    // Both ends of the gradient must be transparent, otherwise the line
    // would be cut off abruptly at top/bottom.
    expect(seam).toMatch(/linear-gradient\(\s*180deg,\s*transparent 0%/);
    expect(seam).toMatch(/transparent 100%\s*\)/);
  });
});
