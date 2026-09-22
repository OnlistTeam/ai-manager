import fs from "node:fs";
import path from "node:path";
import { describe, expect, it } from "vitest";

const ROOT = path.resolve(__dirname, "..", "..", "..", "src");
const CSS_PATH = path.join(ROOT, "index.css");

function ruleBody(css: string, selector: string): string {
  const escaped = selector.replace(/[.*+?^${}()|[\]\\]/g, "\\$&");
  return css.match(new RegExp(`${escaped}\\s*\\{([^}]*)\\}`))?.[1] ?? "";
}

/** Every selector that declares `property`, in source order. */
function declaringSelectors(css: string, property: string): string[] {
  const selectors: string[] = [];
  const rule = /([^{}]+)\{([^}]*)\}/g;
  for (const [, selector, body] of css.matchAll(rule)) {
    if (body.includes(`${property}:`)) selectors.push(selector.trim());
  }
  return selectors;
}

/**
 * Dialogs, menus and the scrim are rendered by React into portals under
 * `document.body`, so they are not descendants of the canvas element and
 * inherit from `<html>` instead.
 *
 * That is the whole reason these tests exist. The route palette used to be
 * declared on `.app-window-canvas`, which meant `hsl(var(--canvas-indigo))`
 * inside a portal referenced a variable that was not in scope: the value was
 * invalid, the browser dropped the declaration, and every dialog and menu in
 * the product came out fully transparent. Nothing in a class name or a diff
 * showed it — the CSS reads as if it paints a fill.
 */
describe("floating surfaces", () => {
  const css = fs.readFileSync(CSS_PATH, "utf8");

  for (const property of [
    "--canvas-violet",
    "--canvas-indigo",
    "--canvas-deep",
  ]) {
    it(`${property} is declared where a portal can reach it`, () => {
      const selectors = declaringSelectors(css, property);
      expect(selectors.length).toBeGreaterThan(0);

      // At least one declaration has to match `<html>` itself, or nothing
      // outside the canvas subtree resolves.
      expect(
        selectors.some(
          (selector) =>
            selector.includes(":root") || selector.startsWith("[data-route="),
        ),
      ).toBe(true);

      // And none may be scoped to the canvas element, which portals are not
      // inside of.
      expect(
        selectors.filter((selector) => selector.includes(".app-window-canvas")),
      ).toEqual([]);
    });
  }

  for (const selector of [".app-floating-surface", ".app-floating-menu"]) {
    it(`${selector} paints an opaque base`, () => {
      const body = ruleBody(css, selector);
      expect(body).not.toBe("");

      const background = body.match(/background-color:\s*([^;]+);/)?.[1];
      expect(background).toBeDefined();
      // An alpha channel on the base is exactly the defect: `hsl(H S L / A)`.
      expect(background).not.toMatch(/\//);
      expect(background).toMatch(/^hsl\(var\(--canvas-(indigo|deep)\)\)$/);

      // Blur only exists to show what is behind. Nothing is.
      expect(body).not.toMatch(/backdrop-filter/);
    });
  }

  it("the menu sits a step lighter than the surface it opens over", () => {
    // Against an identical fill a menu has no edge to read, so the two must
    // not resolve to the same wash.
    expect(ruleBody(css, ".app-floating-surface")).toMatch(/--canvas-deep/);
    expect(ruleBody(css, ".app-floating-menu")).toMatch(/--canvas-indigo/);
  });

  it("the scrim stays translucent, because dimming the page is its job", () => {
    const overlay = ruleBody(css, ".app-modal-overlay");
    // Route-independent on purpose: a scrim removes the page rather than
    // colouring it, and a fixed value cannot go missing in a portal.
    expect(overlay).toMatch(/background:\s*rgb\([\d\s]+\/\s*0\.\d+\)/);
  });
});
