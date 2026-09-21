import { describe, expect, it } from "vitest";
import {
  NEUTRAL_TURNTABLE_X,
  turntableFrameAt,
} from "@/shared/ui/turntableFrame";

const TERMINAL = { frames: 25, cols: 5, rows: 5 } as const;

describe("turntableFrameAt", () => {
  /**
   * This is the starting point of the whole turntable. The CSS fallback
   * value is 0%, i.e. the first frame of the sequence — one extreme of yaw.
   * Any caller that hasn't wired up the pointer would end up with the model
   * twisted off to one side the moment it uses this value.
   */
  it("puts a centred pointer on the middle frame, not the first one", () => {
    const frame = turntableFrameAt(TERMINAL, NEUTRAL_TURNTABLE_X);
    // Frame 12 (0-indexed) = column 3, row 3 — two cells back in each direction.
    expect(frame).toEqual({
      x: "-40%",
      y: "-40%",
      nextX: "-60%",
      nextY: "-40%",
      blend: 0,
    });
  });

  it("turns the object towards the pointer, against the camera's direction", () => {
    // Cursor at the far right → the object turns its front to the right →
    // what we see is the frame where the camera was at the far left.
    expect(turntableFrameAt(TERMINAL, 1)).toMatchObject({ x: "0%", y: "0%" });
    expect(turntableFrameAt(TERMINAL, -1)).toMatchObject({
      x: "-80%",
      y: "-80%",
    });
  });

  it("holds two frames and blends between them instead of snapping", () => {
    // Halfway between center and the far right lands on frame 6; nudging it
    // further should pin frames 6 and 7.
    const between = turntableFrameAt(TERMINAL, 0.45);
    expect(between.blend).toBeGreaterThan(0);
    expect(between.blend).toBeLessThan(1);
    expect(between.nextX).not.toEqual(between.x);
  });

  it("stays inside the sheet at the ends of the sweep", () => {
    for (const x of [-4, -1, 0, 1, 4]) {
      const frame = turntableFrameAt(TERMINAL, x);
      for (const offset of [frame.x, frame.y, frame.nextX, frame.nextY]) {
        const value = Number.parseFloat(offset);
        expect(value).toBeLessThanOrEqual(0);
        expect(value).toBeGreaterThanOrEqual(-80);
      }
      expect(frame.blend).toBeGreaterThanOrEqual(0);
      expect(frame.blend).toBeLessThan(1);
    }
  });

  it("keeps the last frame from pointing past the end of the sheet", () => {
    // With an even frame count, center falls between two frames; the earlier
    // frame must not overflow past the end of the sequence.
    const even = turntableFrameAt({ frames: 8, cols: 4, rows: 2 }, -1);
    expect(even).toMatchObject({ x: "-75%", nextX: "-75%", blend: 0 });
  });
});
