import type { ToolId } from "@/entities/routing";
import type { FlightTone } from "./flightPlan";
import { toolHue } from "./stageModel";

/**
 * The few DOM touches the flight player needs: finding a wire and a dot,
 * moving the dot, and a brief glow on the tool that sent. Nothing here
 * reads layout except path lengths, which do not force a reflow.
 */
export function findWire(root: Element, wire: string): SVGPathElement | null {
  for (const path of root.querySelectorAll<SVGPathElement>("path[data-wire]")) {
    if (path.getAttribute("data-wire") === wire) return path;
  }
  return null;
}

export function findDot(root: Element, seq: number): SVGCircleElement | null {
  return root.querySelector<SVGCircleElement>(
    `circle[data-flight-dot="${seq}"]`,
  );
}

export function wireLength(path: SVGPathElement): number {
  if (typeof path.getTotalLength !== "function" || !path.getAttribute("d")) {
    return 0;
  }
  return path.getTotalLength();
}

export function placeDot(
  dot: SVGCircleElement,
  path: SVGPathElement,
  length: number,
  reverse: boolean,
  progress: number,
): void {
  const point = path.getPointAtLength(
    length * (reverse ? 1 - progress : progress),
  );
  dot.setAttribute("cx", point.x.toFixed(1));
  dot.setAttribute("cy", point.y.toFixed(1));
  dot.setAttribute("visibility", "visible");
}

export function toneFill(tone: FlightTone, tool: ToolId): string {
  if (tone === "success") return "hsl(var(--ui-success))";
  if (tone === "failure") return "hsl(var(--ui-danger))";
  return `hsl(${toolHue(tool)})`;
}

export function glowTool(root: Element, tool: ToolId): void {
  const node = root.querySelector<HTMLElement>(`[data-stage-tool="${tool}"]`);
  if (typeof node?.animate !== "function") return;
  const hue = toolHue(tool);
  node.animate(
    [
      { boxShadow: `0 0 0 0 hsl(${hue} / 0.55)` },
      { boxShadow: `0 0 0 7px hsl(${hue} / 0)` },
    ],
    { duration: 900, easing: "ease-out" },
  );
}
