/**
 * Layout of the turntable sequence. With this, yaw is no longer faked by
 * CSS-rotating a single image; instead it switches to the frame actually
 * rendered from that angle — so the side walls, top face, and base top
 * genuinely show and hide with the angle.
 */
export interface SpatialTurntable {
  /** Total frame count in the sequence, evenly covering both the positive and negative ends of yaw. */
  frames: number;
  /** The sprite sheet's column and row count. */
  cols: number;
  rows: number;
}

/** The two frames pinned at a given yaw position, and the cross-fade ratio between them. */
export interface TurntableFrame {
  /** The current frame's offset within the strip, a percentage string fed directly into `translate3d`. */
  x: string;
  y: string;
  /** The next frame's offset. */
  nextX: string;
  nextY: string;
  /** 0..1. */
  blend: number;
}

/** The horizontal offset when the pointer rests dead center. */
export const NEUTRAL_TURNTABLE_X = 0;

/**
 * Pointer horizontal offset (−1..1, 0 is dead center) → which frame to show.
 *
 * The sequence is rendered by yawing the camera around the object, but the
 * interaction wants the object to turn toward the cursor — the two directions
 * are opposite: cursor on the right means the object turns its front to the
 * right, so what we see is its left side, i.e. the frame where the camera was
 * on the left. Without this inversion, the model would obviously turn its
 * back to the mouse.
 *
 * The frame number is deliberately not rounded. Rounding would make it jump
 * frame by frame — the pointer moves continuously, but the object would hard
 * jump every 2°, and that jump, layered on top of the simultaneous smooth
 * translation, reads as "jittering." So this pins the two adjacent frames and
 * hands the fractional part up to the caller for cross-fading, turning the
 * discrete sequence back into a continuous angle.
 */
export function turntableFrameAt(
  turntable: SpatialTurntable,
  x: number,
): TurntableFrame {
  const { frames, cols, rows } = turntable;
  const exact = Math.min(frames - 1, Math.max(0, ((1 - x) / 2) * (frames - 1)));
  const lower = Math.floor(exact);
  const upper = Math.min(frames - 1, lower + 1);
  // The strip is cols × container-width wide, so shifting left by one cell is
  // exactly 1 / cols of its own width.
  const offsetX = (index: number) => `${(-(index % cols) * 100) / cols}%`;
  const offsetY = (index: number) =>
    `${(-Math.floor(index / cols) * 100) / rows}%`;
  return {
    x: offsetX(lower),
    y: offsetY(lower),
    nextX: offsetX(upper),
    nextY: offsetY(upper),
    blend: exact - lower,
  };
}
