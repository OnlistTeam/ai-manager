import fs from "node:fs";
import path from "node:path";
import { describe, expect, it } from "vitest";

const ROOT = path.resolve(__dirname, "..", "..", "..", "src");
const CSS_PATH = path.join(ROOT, "index.css");

function ruleBody(css: string, selector: string): string {
  const escaped = selector.replace(/[.*+?^${}()|[\]\\]/g, "\\$&");
  return css.match(new RegExp(`${escaped}\\s*\\{([^}]*)\\}`))?.[1] ?? "";
}

/**
 * A dialog is the one surface that has the user's whole attention, so it may
 * not let the page through. These were glass — `canvas-deep` at 88% with a
 * 34px backdrop blur — and over the route gradient the result was a panel you
 * could read the page through; the menu opened inside one was worse, because
 * it showed the dialog's own labels behind the list.
 *
 * Opacity is checked at the source rule rather than by rendering, because that
 * is where it can silently regress: an `hsl(... / 0.88)` looks deliberate in a
 * diff and is invisible in a test that only asserts a class name.
 */
describe("floating surfaces", () => {
  const css = fs.readFileSync(CSS_PATH, "utf8");

  for (const selector of [".app-floating-surface", ".app-floating-menu"]) {
    it(`${selector} paints an opaque base`, () => {
      const body = ruleBody(css, selector);
      expect(body).not.toBe("");

      const background = body.match(/background-color:\s*([^;]+);/)?.[1];
      expect(background).toBeDefined();
      // An alpha channel on the base is exactly the defect: `hsl(H S L / A)`.
      expect(background).not.toMatch(/\//);
      expect(background).toMatch(/^hsl\(var\(--canvas-(indigo|violet)\)\)$/);

      // Blur only exists to show what is behind. Nothing is.
      expect(body).not.toMatch(/backdrop-filter/);
    });
  }

  it("the menu sits a step lighter than the dialog it opens over", () => {
    // Against an identical surface a menu has no edge to read, so the two must
    // not resolve to the same wash.
    const dialog = ruleBody(css, ".app-floating-surface");
    const menu = ruleBody(css, ".app-floating-menu");
    expect(dialog).toMatch(/--canvas-indigo/);
    expect(menu).toMatch(/--canvas-violet/);
  });

  it("the scrim stays translucent, because dimming the page is its job", () => {
    const overlay = ruleBody(css, ".app-modal-overlay");
    expect(overlay).toMatch(/hsl\(var\(--canvas-deep\)\s*\/\s*0\.\d+\)/);
  });
});
