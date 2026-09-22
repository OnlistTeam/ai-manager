import fs from "node:fs";
import path from "node:path";
import zlib from "node:zlib";
import { describe, expect, it } from "vitest";

const ROOT = path.resolve(__dirname, "..", "..", "src");
const CSS_PATH = path.join(ROOT, "index.css");

function ruleBody(css: string, selector: string): string {
  const escaped = selector.replace(/[.*+?^${}()|[\]\\]/g, "\\$&");
  return css.match(new RegExp(`${escaped}\\s*\\{([^}]*)\\}`))?.[1] ?? "";
}

const ACTIVE_PAGE_VISUALS = {
  "pages/home/EnvironmentHero.tsx": 'model="environment"',
} as const;

const COMPACT_MANAGEMENT_PAGES = [
  "pages/services/ServicesPage.tsx",
  "pages/extensions/ExtensionsPage.tsx",
  "pages/routing/RoutingPage.tsx",
  "pages/usage/UsagePage.tsx",
  "pages/sessions/SessionsPage.tsx",
  "pages/data/DataPage.tsx",
  "pages/settings/SettingsPage.tsx",
] as const;

const SHIPPED_MODELS = [
  "environment",
  "tools",
  "services",
  "extensions",
  "settings",
] as const;

const ARTWORK_ROOT = path.join(ROOT, "assets", "spatial", "models", "v5");

/**
 * The product has exactly one look: the home hero, sidebar mark, and app
 * icon all come from the same 3D model. `mark.png` is only ever shown at
 * 36px in the sidebar, so bundling the 1024px source image would be pure
 * waste; re-run `v5/terminal.blend.py` (see v5/README.md) when a different
 * size is needed.
 */
const SHIPPED_ARTWORK = {
  "terminal.png": { maxSize: 1024, minBytes: 100_000 },
  "mark.png": { maxSize: 256, minBytes: 10_000 },
} as const;

/** Grid of the turntable sprite sheet; must match src/pages/home/terminalModel.ts. */
const TURNTABLE = { frames: 25, cols: 5, rows: 5 } as const;

interface DecodedPng {
  width: number;
  pixels: Buffer;
}

/**
 * Good-enough PNG decoding: 8-bit RGBA, non-interlaced — the kind both
 * Pillow and `tauri icon` produce. This exists only for the sync check
 * below; not worth pulling in an image library for.
 */
function decodePng(bytes: Buffer): DecodedPng {
  const width = bytes.readUInt32BE(16);
  const height = bytes.readUInt32BE(20);
  const chunks: Buffer[] = [];
  for (let at = 8; at + 8 <= bytes.length; ) {
    const length = bytes.readUInt32BE(at);
    if (bytes.subarray(at + 4, at + 8).toString("ascii") === "IDAT") {
      chunks.push(bytes.subarray(at + 8, at + 8 + length));
    }
    at += 12 + length;
  }

  const raw = zlib.inflateSync(Buffer.concat(chunks));
  const bpp = 4;
  const stride = width * bpp;
  const pixels = Buffer.alloc(height * stride);
  for (let y = 0; y < height; y++) {
    const filter = raw[y * (stride + 1)];
    const line = raw.subarray(y * (stride + 1) + 1, (y + 1) * (stride + 1));
    for (let x = 0; x < stride; x++) {
      const left = x >= bpp ? pixels[y * stride + x - bpp] : 0;
      const up = y > 0 ? pixels[(y - 1) * stride + x] : 0;
      const corner = x >= bpp && y > 0 ? pixels[(y - 1) * stride + x - bpp] : 0;
      let value = line[x];
      if (filter === 1) value += left;
      else if (filter === 2) value += up;
      else if (filter === 3) value += (left + up) >> 1;
      else if (filter === 4) {
        const guess = left + up - corner;
        const dl = Math.abs(guess - left);
        const du = Math.abs(guess - up);
        const dc = Math.abs(guess - corner);
        value += dl <= du && dl <= dc ? left : du <= dc ? up : corner;
      }
      pixels[y * stride + x] = value & 0xff;
    }
  }
  return { width, pixels };
}

/** Box-downsample to n×n, compositing transparent pixels against mid-gray — the padding around an icon is excluded from the comparison. */
function thumbnail({ width, pixels }: DecodedPng, n = 24): Float64Array {
  const out = new Float64Array(n * n * 3);
  const cell = width / n;
  for (let ty = 0; ty < n; ty++) {
    for (let tx = 0; tx < n; tx++) {
      const channels = [0, 0, 0];
      let count = 0;
      for (
        let y = Math.floor(ty * cell);
        y < Math.floor((ty + 1) * cell);
        y++
      ) {
        for (
          let x = Math.floor(tx * cell);
          x < Math.floor((tx + 1) * cell);
          x++
        ) {
          const i = (y * width + x) * 4;
          const alpha = pixels[i + 3] / 255;
          for (let c = 0; c < 3; c++) {
            channels[c] += pixels[i + c] * alpha + 128 * (1 - alpha);
          }
          count++;
        }
      }
      for (let c = 0; c < 3; c++)
        out[(ty * n + tx) * 3 + c] = channels[c] / count;
    }
  }
  return out;
}

function meanChannelDelta(a: Float64Array, b: Float64Array): number {
  let total = 0;
  for (let i = 0; i < a.length; i++) total += Math.abs(a[i] - b[i]);
  return total / a.length;
}

describe("page spatial visual contract", () => {
  it("keeps one route-specific spatial model on every declared visual page", () => {
    for (const [file, model] of Object.entries(ACTIVE_PAGE_VISUALS)) {
      const source = fs.readFileSync(path.join(ROOT, file), "utf8");
      expect(source, file).toContain(model);
    }
  });

  it("keeps management pages compact instead of restoring a framed spatial hero", () => {
    for (const file of COMPACT_MANAGEMENT_PAGES) {
      const source = fs.readFileSync(path.join(ROOT, file), "utf8");
      expect(source, file).toContain("<SectionHeader");
      expect(source, file).not.toContain("<SpatialPageHeader");
      expect(source, file).not.toMatch(/model="[^"]+"/);
    }
  });

  it("ships every packaged artwork as a bounded square transparent PNG", () => {
    for (const [file, limits] of Object.entries(SHIPPED_ARTWORK)) {
      const bytes = fs.readFileSync(path.join(ARTWORK_ROOT, file));

      expect(bytes.subarray(1, 4).toString("ascii"), file).toBe("PNG");
      const width = bytes.readUInt32BE(16);
      const height = bytes.readUInt32BE(20);
      expect(width, file).toBe(height);
      expect(width, file).toBeLessThanOrEqual(limits.maxSize);
      // PNG IHDR colour type 6 = truecolour with alpha.
      expect(bytes[25], file).toBe(6);
      expect(bytes.byteLength, file).toBeGreaterThan(limits.minBytes);
    }
  });

  /**
   * The platform icons are derivatives generated from app-icon.png, and
   * `pnpm tauri icon` is a manual, one-off command. Forgetting to run it
   * fails silently: the repo has the new source image, but the dock still
   * shows the old icon — one of the hardest kinds of bug to catch yourself,
   * because you believe you already made the change.
   *
   * The comparison downsamples both images to 24x24 and compares the mean
   * channel delta. The same source image round-tripped through different
   * encoders and sizes measures 0.3-0.9 in practice; a previous icon
   * revision with a different backplate colour and subject size measured
   * 7.8 — an order of magnitude apart. Resampling loss at 128px and below
   * is too large (1.7-3.6) to be useful, so those sizes are excluded.
   */
  it("keeps the shipped platform icons in sync with the icon source", () => {
    const source = thumbnail(
      decodePng(fs.readFileSync(path.join(ARTWORK_ROOT, "app-icon.png"))),
    );
    const iconsRoot = path.resolve(__dirname, "..", "..", "src-tauri", "icons");

    for (const file of [
      "icon.png",
      "128x128@2x.png",
      "Square310x310Logo.png",
    ]) {
      const shipped = thumbnail(
        decodePng(fs.readFileSync(path.join(iconsRoot, file))),
      );
      expect(meanChannelDelta(source, shipped), file).toBeLessThan(2);
    }
  });

  /**
   * One product, one look: the hero and the sidebar mark must come from the
   * same model, otherwise the user sees two unrelated things on the same
   * screen both claiming to be this app. Old asset sets should not linger
   * in the repository.
   */
  it("keeps only the current model set in the repository", () => {
    const modelsRoot = path.join(ROOT, "assets", "spatial", "models");

    expect(fs.readdirSync(modelsRoot).sort()).toEqual(["v5"]);
  });

  /**
   * The home model is a turntable frame sequence, not a single spinning
   * image. That distinction is easy to lose in a later "simplification"
   * pass, and the page still looks fine afterward — except the side of the
   * object never comes into view when it turns, because a flat texture has
   * no side to show. This test pins that behaviour down.
   */
  it("turns the home model by swapping rendered frames, not by rotating one picture", () => {
    const model = fs.readFileSync(
      path.join(ROOT, "pages/home/terminalModel.ts"),
      "utf8",
    );
    const hero = fs.readFileSync(
      path.join(ROOT, "pages/home/EnvironmentHero.tsx"),
      "utf8",
    );

    expect(hero).toContain("turntable={TERMINAL_TURNTABLE}");
    expect(hero).not.toMatch(/artwork=/);

    // The frame index must not be rounded. Rounding would hard-cut to the
    // next cell every 2.2°, and that cut, layered on top of the simultaneous
    // smooth shift, reads as jitter; cross-fading between adjacent frames is
    // what turns the discrete sequence back into a continuous angle.
    const pointer = fs.readFileSync(
      path.join(ROOT, "shared/ui/useSpatialPointer.ts"),
      "utf8",
    );
    expect(pointer).not.toMatch(/Math\.round\(\(\(1 - vector\.x\)/);
    expect(pointer).toContain("--spatial-frame-next-x");
    expect(pointer).toContain("--spatial-frame-blend");
    // The pointer hook must know the layout, otherwise it only writes a CSS
    // rotation angle and the frame sequence goes to waste.
    expect(hero).toContain(
      "useSpatialPointer<HTMLDivElement>(TERMINAL_TURNTABLE)",
    );
    for (const [key, value] of Object.entries(TURNTABLE)) {
      expect(model, key).toMatch(new RegExp(`${key}:\\s*${value}`));
    }
  });

  /**
   * The sheet's actual canvas and the grid declared in terminalModel.ts must
   * agree. Changing either side alone makes the page pick the wrong cell —
   * and silently: the image is still there, just at the wrong angle.
   */
  it("keeps the sprite sheet a whole number of cells in both directions", () => {
    const bytes = fs.readFileSync(
      path.join(ARTWORK_ROOT, "terminal-turntable.webp"),
    );

    expect(bytes.subarray(0, 4).toString("ascii")).toBe("RIFF");
    expect(bytes.subarray(8, 12).toString("ascii")).toBe("WEBP");
    // The VP8X extended-format chunk header carries the canvas size (3 bytes
    // little-endian each, stored as size - 1).
    expect(bytes.subarray(12, 16).toString("ascii")).toBe("VP8X");
    const width = bytes.readUIntLE(24, 3) + 1;
    const height = bytes.readUIntLE(27, 3) + 1;

    // Each cell must be a whole number of pixels, and equal in width and
    // height; otherwise shifting by one cell accumulates a half-pixel
    // offset that grows worse with every later frame. The cell includes the
    // sampling safety margin left by pack-turntable.py.
    expect(
      width % TURNTABLE.cols,
      "sheet width is not a whole multiple of the column count",
    ).toBe(0);
    expect(
      height % TURNTABLE.rows,
      "sheet height is not a whole multiple of the row count",
    ).toBe(0);
    expect(width / TURNTABLE.cols).toBe(height / TURNTABLE.rows);
    expect(TURNTABLE.frames).toBeLessThanOrEqual(
      TURNTABLE.cols * TURNTABLE.rows,
    );
    expect(bytes.byteLength).toBeLessThan(600_000);
  });

  it("derives the sidebar mark from the very model the hero shows", () => {
    const shell = fs.readFileSync(path.join(ROOT, "app/AppShell.tsx"), "utf8");
    const model = fs.readFileSync(
      path.join(ROOT, "pages/home/terminalModel.ts"),
      "utf8",
    );
    expect(shell).toContain("models/v5/mark.png");
    expect(model).toContain("models/v5/terminal-turntable.webp");
  });

  it("keeps pointer movement out of React state", () => {
    const source = fs.readFileSync(
      path.join(ROOT, "shared/ui/useSpatialPointer.ts"),
      "utf8",
    );
    expect(source).not.toContain("useState");
    expect(source).toContain("requestAnimationFrame");
    expect(source).toContain("style.setProperty");
    expect(source).toContain("prefers-reduced-motion: reduce");
    // The model follows the cursor across the whole window, measured from the
    // artwork's own centre — not from whichever card happens to be hovered.
    expect(source).toContain('window.addEventListener("pointermove"');
    expect(source).toContain("[data-spatial-origin]");
  });

  it("keeps distinct geometry and a gallery sample for every route model", () => {
    const css = fs.readFileSync(CSS_PATH, "utf8");
    const gallery = fs.readFileSync(
      path.join(ROOT, "shared/ui/__gallery__/sections/ProductSection.tsx"),
      "utf8",
    );

    for (const model of SHIPPED_MODELS) {
      expect(css, model).toContain(
        `[data-model="${model}"] .spatial-scene__motif`,
      );
      expect(css, model).toContain(
        `[data-model="${model}"] .spatial-scene__sculpture`,
      );
      expect(gallery, model).toContain(`model: "${model}"`);
    }
  });

  it("keeps the spatial focal point while compacting short desktop windows", () => {
    const css = fs.readFileSync(CSS_PATH, "utf8");

    expect(css).toContain("@media (max-height: 760px) and (min-width: 760px)");
    expect(css).toContain("@media (max-height: 650px) and (min-width: 760px)");
    expect(css).toContain("@container spatial-page (min-width: 580px)");
    expect(css).toContain("@container spatial-page (min-width: 820px)");
    expect(css).toContain(".spatial-page-hero.environment-hero");
    expect(css).toContain("--spatial-scene-size: 178px");
    expect(css).toContain("@container metric-strip (min-width: 560px)");
    expect(css).toContain(
      "grid-template-columns: minmax(210px, 0.9fr) minmax(270px, 1.1fr)",
    );

    const home = fs.readFileSync(
      path.join(ROOT, "pages/home/EnvironmentHero.tsx"),
      "utf8",
    );
    const homeSkeleton = fs.readFileSync(
      path.join(ROOT, "pages/home/EnvironmentHeroSkeleton.tsx"),
      "utf8",
    );
    expect(home).toContain("environment-hero__layout");
    expect(home).not.toContain("environment-hero__recommendation-description");
    expect(homeSkeleton).toContain("environment-hero__layout");
  });

  it("moves the stage, object and ground on separate depth planes", () => {
    const css = fs.readFileSync(CSS_PATH, "utf8");

    expect(css).toContain(".spatial-page-hero__lines");
    expect(css).toContain("calc(0px - var(--spatial-shift-x))");
    expect(css).toContain(".spatial-scene__floor");
    expect(css).toContain(".spatial-scene__grid");
    expect(css).toContain(".spatial-scene__core::before");
    expect(css).toContain(".spatial-scene__core::after");
    expect(css).toContain(".spatial-scene__sculpture");
    expect(css).toContain(".spatial-scene__artwork-shell");
    expect(css).toContain(".spatial-scene__artwork-glint");
    expect(css).toContain("--spatial-detail-x");
  });

  /**
   * A cast shadow on the ground is the strongest evidence that "this is a
   * physical object" — provided it stays on the ground. The moment it's
   * placed inside the rotating plane, it turns together with the object —
   * and a shadow that rotates along with the thing casting it is exactly
   * the giveaway that "this is actually just a flat image." So the close
   * contact shading stays on artwork-shell, while the cast shadow must be
   * drawn by the ground layer outside the tilt transform, and slide in the
   * opposite direction to the pointer.
   */
  it("leaves the cast shadow on the ground while the object turns above it", () => {
    const css = fs.readFileSync(CSS_PATH, "utf8");
    const shell = ruleBody(css, ".spatial-scene__artwork-shell");
    const floor = ruleBody(
      css,
      '.spatial-scene[data-artwork="true"] .spatial-scene__floor',
    );

    expect(shell.match(/drop-shadow/g) ?? []).toHaveLength(1);
    expect(floor).toContain("var(--spatial-shadow)");
    expect(floor).toContain("calc(0px - var(--spatial-shift-x, 0px) * 0.7)");
  });

  it("fades the model light field on every edge instead of drawing a rectangular stage", () => {
    const css = fs.readFileSync(CSS_PATH, "utf8");
    const wash = ruleBody(css, ".spatial-page-hero__wash");
    const hero = ruleBody(css, ".spatial-page-hero");

    expect(hero).toContain("border-radius: 0");
    expect(hero).toContain("background: transparent !important");
    expect(hero).toContain("box-shadow: none !important");
    expect(css).toContain("inset-block: -12%;\n  inset-inline: 0;");
    expect(css).not.toContain("inset: -12%;");
    expect(wash).toContain("mask-image: radial-gradient");
    expect(wash).toContain("transparent 82%");
    expect(
      css.match(/circle at var\(--spatial-art-x\) var\(--spatial-art-y\)/g) ??
        [],
    ).toHaveLength(1);
    expect(css).toMatch(
      /\.spatial-page-hero__lines \{\s+z-index: -1;[\s\S]*?mask-image: radial-gradient/,
    );
    expect(css).toMatch(
      /\.spatial-page-hero__lines \{\s+z-index: -1;\s+opacity: 0\.12/,
    );
  });

  it("gives every destination its own continuous-canvas light palette", () => {
    const css = fs.readFileSync(CSS_PATH, "utf8");
    const routes = [
      "home",
      "tools",
      "services",
      "extensions",
      "mcp",
      "prompts",
      "data",
      "settings",
    ];
    const primaryColours = new Set<string>();

    for (const route of routes) {
      // Scoped to `[data-route]` rather than to the canvas element, because
      // AppShell mirrors the route onto `<html>` so dialogs and menus — which
      // render into portals outside the canvas — can still resolve the palette.
      const body = ruleBody(css, `[data-route="${route}"]`);
      expect(body, route).toContain("--route-primary:");
      expect(body, route).toContain("--route-secondary:");
      expect(body, route).toContain("--route-bloom:");
      expect(body, route).toContain("--canvas-violet:");
      primaryColours.add(body.match(/--route-primary:\s*([^;]+);/)?.[1] ?? "");
    }

    expect(primaryColours.size).toBe(routes.length);
    const windowCanvas = ruleBody(css, ".app-window-canvas");
    expect(windowCanvas).toContain("border-radius: 0");
    expect(windowCanvas).toContain("user-select: none");
    expect(windowCanvas).toContain("linear-gradient(\n      170deg");
    expect(windowCanvas).not.toContain("at -7% 52%");
    expect(ruleBody(css, "body")).toContain("background: transparent");
    expect(css).toContain(".app-route-backdrop__layer--incoming");
    expect(css).toContain(".app-route-backdrop__layer--outgoing");
    // A colour change is a dissolve, not a slide: the backdrop layers carry no
    // direction and cross-fade at a constant rate, so one hue reads as
    // travelling evenly into the next.
    expect(css).not.toContain("--route-backdrop-enter-y");
    expect(css).toContain(
      "animation: route-backdrop-in var(--motion-page) linear",
    );
    expect(css).toContain(
      "animation: route-backdrop-out var(--motion-page) linear",
    );
    expect(css).toContain("animation: route-content-in var(--motion-page)");
    expect(css).toContain("animation: route-content-out var(--motion-page)");
    expect(css).toContain("--page-enter-y: 88px");
    expect(css).toContain("--page-enter-y: -88px");
    expect(ruleBody(css, ".spatial-page-hero__wash")).toContain(
      "var(--route-secondary",
    );
    expect(css).toMatch(
      /\.app-window-canvas\s+:is\([^)]*\[data-selectable-text\][^)]*\)\s*\{[^}]*user-select: text;/,
    );
  });

  it("uses one perspective tilt plane without nested 3D containers", () => {
    const css = fs.readFileSync(CSS_PATH, "utf8");

    for (const selector of [
      ".spatial-scene",
      ".spatial-scene__tilt",
      ".spatial-scene__object",
      ".spatial-scene__core",
      ".spatial-scene__orbit",
      ".spatial-scene__motif",
      ".spatial-scene__motif-part",
      ".spatial-scene__sculpture",
      ".spatial-scene__sculpture-piece",
    ]) {
      const body = ruleBody(css, selector);
      expect(body, selector).not.toBe("");
      expect(body, selector).not.toContain("transform-style: preserve-3d");
    }

    // A camera distance in raw pixels is only correct at one scene size. The
    // hero is the largest instance, so a fixed 820px viewed it almost
    // orthographically and rotation produced no convergence at all.
    expect(ruleBody(css, ".spatial-scene")).toContain(
      "perspective: calc(var(--spatial-scene-size) * 1.55)",
    );
    expect(ruleBody(css, ".spatial-scene__tilt")).toContain(
      "rotateX(var(--spatial-rotate-x, 0deg))",
    );
    expect(ruleBody(css, ".spatial-scene__tilt")).toContain(
      "rotateY(var(--spatial-rotate-y, 0deg))",
    );
  });

  it("pauses decorative spatial motion while the desktop window is inactive", () => {
    const css = fs.readFileSync(CSS_PATH, "utf8");
    const inactiveMotionStart = css.indexOf(
      'html[data-window-active="false"] .spatial-scene__object',
    );
    const reducedMotionStart = css.indexOf(
      "@media (prefers-reduced-motion: reduce)",
      inactiveMotionStart,
    );
    const inactiveMotionRules = css.slice(
      inactiveMotionStart,
      reducedMotionStart,
    );

    expect(inactiveMotionStart).toBeGreaterThan(-1);
    expect(reducedMotionStart).toBeGreaterThan(inactiveMotionStart);
    expect(inactiveMotionRules).toContain(".spatial-scene__orbit");
    expect(inactiveMotionRules).toContain(".spatial-scene__shard");
    expect(inactiveMotionRules).toContain(
      '.spatial-scene[data-loading="true"]',
    );
    expect(inactiveMotionRules).toContain("animation-play-state: paused");
  });
});
