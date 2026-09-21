import terminalTurntable from "@/assets/spatial/models/v5/terminal-turntable.webp";

/**
 * The turntable sequence for the home page model. The layout must match the
 * actual grid of `v5/terminal-turntable.webp`; the asset is rendered by
 * `v5/terminal.blend.py`, so changing the frame count or grid means changing
 * both sides together.
 *
 * Exported as a module-level constant for two reasons: it's the same thing
 * shared by the hero, the skeleton, and the readiness notice, and writing a
 * separate copy in each would eventually drift; also, it's passed into
 * useSpatialPointer as a dependency, and creating a new object on every
 * render would force all the callbacks inside the hook to be rebuilt.
 */
export const TERMINAL_TURNTABLE = {
  sheet: terminalTurntable,
  frames: 25,
  cols: 5,
  rows: 5,
} as const;
