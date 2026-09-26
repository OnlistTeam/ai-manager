/**
 * Wire geometry for the live stage: plain boxes measured relative to the
 * stage in, SVG path data out. Kept free of the DOM so it can be tested.
 */
export interface StageBox {
  left: number;
  top: number;
  right: number;
  bottom: number;
}

export interface StagePoint {
  x: number;
  y: number;
}

export interface StageWire {
  id: string;
  d: string;
}

export interface StageLayout {
  hub: StageBox;
  /** Nodes on the left; their wires run from the right edge to the hub. */
  sources: readonly { wire: string; box: StageBox }[];
  /** Rows on the right; their wires run from the hub to the left edge. */
  targets: readonly { wire: string; box: StageBox }[];
}

const round = (value: number) => Math.round(value * 10) / 10;

function middleY(box: StageBox): number {
  return (box.top + box.bottom) / 2;
}

/**
 * A smooth S-curve that leaves and arrives horizontally, so wires fan out of
 * a node and into the next one without kinks.
 */
export function curveBetween(from: StagePoint, to: StagePoint): string {
  const bend = round(from.x + (to.x - from.x) / 2);
  const [x1, y1, x2, y2] = [from.x, from.y, to.x, to.y].map(round);
  return `M${x1} ${y1} C${bend} ${y1} ${bend} ${y2} ${x2} ${y2}`;
}

/**
 * Wires run between node edges: into the hub's left side from each tool and
 * out of its right side to each service. The nodes are translucent, so a
 * wire never runs underneath one; a dot entering the hub disappears into AI
 * Manager and the next leg brings it out on the other side.
 */
export function stageWires(layout: StageLayout): StageWire[] {
  const { hub } = layout;
  const hubY = middleY(hub);
  return [
    ...layout.sources.map(({ wire, box }) => ({
      id: wire,
      d: curveBetween(
        { x: box.right, y: middleY(box) },
        { x: hub.left, y: hubY },
      ),
    })),
    ...layout.targets.map(({ wire, box }) => ({
      id: wire,
      d: curveBetween(
        { x: hub.right, y: hubY },
        { x: box.left, y: middleY(box) },
      ),
    })),
  ];
}

/** Eased progress along a leg: slow out, glide, slow in. */
export function easeInOut(progress: number): number {
  const clamped = Math.min(1, Math.max(0, progress));
  return (1 - Math.cos(Math.PI * clamped)) / 2;
}
