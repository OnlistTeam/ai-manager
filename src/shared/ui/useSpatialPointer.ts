import { useCallback, useEffect, useRef } from "react";
import { turntableFrameAt, type SpatialTurntable } from "./turntableFrame";

interface SpatialPointerBinding<T extends HTMLElement> {
  ref: React.RefObject<T>;
}

interface PointerVector {
  x: number;
  y: number;
}

/**
 * How far the pointer travels, in pixels, before the scene reaches full tilt.
 * Measured from the artwork's own centre, so the model keeps turning towards
 * the cursor while it is anywhere near the stage rather than only inside it.
 */
const REACH = 620;

/*
 * Pitch stays small on purpose. The model is an object standing on a floor: it
 * can turn to face you, but tipping it far forward and back swings its base
 * through the air and it stops reading as something resting on a surface.
 *
 * Yaw is ±26° either way. Without a turntable it is a CSS rotation of a flat
 * picture, which is all a single raster can do. With one, the CSS yaw is left
 * at zero and the angle is served by the frame that was actually rendered from
 * it — rotating the sprite as well would turn the object twice.
 */
const YAW_RANGE = 26;

function writeVector(
  node: HTMLElement,
  vector: PointerVector,
  turntable?: SpatialTurntable,
): void {
  node.style.setProperty("--spatial-rotate-x", `${vector.y * -12}deg`);
  node.style.setProperty(
    "--spatial-rotate-y",
    turntable ? "0deg" : `${vector.x * YAW_RANGE}deg`,
  );
  node.style.setProperty("--spatial-shift-x", `${vector.x * 10}px`);
  node.style.setProperty("--spatial-shift-y", `${vector.y * 15}px`);
  node.style.setProperty("--spatial-detail-x", `${vector.x * 6}px`);
  node.style.setProperty("--spatial-detail-y", `${vector.y * 9}px`);
  node.style.setProperty("--spatial-glint-x", `${vector.x * 20}px`);
  node.style.setProperty("--spatial-glint-y", `${vector.y * 14}px`);
  node.style.setProperty("--spatial-stage-glow-x", `${22 + vector.x * 13}%`);
  node.style.setProperty("--spatial-stage-glow-y", `${38 + vector.y * 14}%`);
  node.style.setProperty("--spatial-reflection-x", `${28 + vector.x * 22}%`);
  node.style.setProperty("--spatial-reflection-y", `${18 + vector.y * 14}%`);

  if (!turntable) return;
  const frame = turntableFrameAt(turntable, vector.x);
  node.style.setProperty("--spatial-frame-x", frame.x);
  node.style.setProperty("--spatial-frame-y", frame.y);
  node.style.setProperty("--spatial-frame-next-x", frame.nextX);
  node.style.setProperty("--spatial-frame-next-y", frame.nextY);
  node.style.setProperty("--spatial-frame-blend", `${frame.blend}`);
}

/** The artwork itself is the origin; the stage around it is the fallback. */
function originOf(stage: HTMLElement): DOMRect {
  const scene = stage.querySelector<HTMLElement>("[data-spatial-origin]");
  return (scene ?? stage).getBoundingClientRect();
}

/**
 * Turns the scene towards the pointer from anywhere in the window.
 *
 * The listener is on `window`, not on the stage: the model should follow the
 * cursor while the user reads a paragraph or reaches for a button far below it, which is what makes the object feel present in the room rather than
 * being a hover effect on one card. Values are written straight to CSS custom
 * properties, so no pointer coordinate ever enters React state.
 */
export function useSpatialPointer<T extends HTMLElement>(
  turntable?: SpatialTurntable,
): SpatialPointerBinding<T> {
  const ref = useRef<T>(null);
  const pending = useRef<PointerVector>({ x: 0, y: 0 });
  const frame = useRef<number | null>(null);
  const reducedMotion = useRef(false);

  const flush = useCallback(() => {
    frame.current = null;
    if (ref.current) writeVector(ref.current, pending.current, turntable);
  }, [turntable]);

  const schedule = useCallback(() => {
    if (frame.current !== null) return;
    if (typeof window === "undefined" || !window.requestAnimationFrame) {
      flush();
      return;
    }
    frame.current = window.requestAnimationFrame(flush);
  }, [flush]);

  const resetPointer = useCallback(() => {
    pending.current = { x: 0, y: 0 };
    if (
      frame.current !== null &&
      typeof window !== "undefined" &&
      typeof window.cancelAnimationFrame === "function"
    ) {
      window.cancelAnimationFrame(frame.current);
      frame.current = null;
    }
    if (ref.current) writeVector(ref.current, pending.current, turntable);
  }, [turntable]);

  // Settle on the neutral frame as soon as it mounts, without waiting for the
  // pointer's first move. SpatialScene also writes its own neutral starting
  // pose (only it knows its own grid); together the two cover both cases:
  // this one handles a stage that already has the pointer wired up, and that
  // one handles a caller that never wires up the pointer at all.
  useEffect(() => {
    if (ref.current) writeVector(ref.current, { x: 0, y: 0 }, turntable);
  }, [turntable]);

  useEffect(() => {
    if (typeof window === "undefined") return undefined;

    const onPointerMove = (event: PointerEvent) => {
      const stage = ref.current;
      if (!stage || reducedMotion.current) return;
      if (event.pointerType === "touch") return;
      const rect = originOf(stage);
      if (rect.width === 0 || rect.height === 0) return;

      const centreX = rect.left + rect.width / 2;
      const centreY = rect.top + rect.height / 2;
      pending.current = {
        x: Math.max(-1, Math.min(1, (event.clientX - centreX) / REACH)),
        y: Math.max(-1, Math.min(1, (event.clientY - centreY) / REACH)),
      };
      schedule();
    };

    window.addEventListener("pointermove", onPointerMove, { passive: true });
    return () => window.removeEventListener("pointermove", onPointerMove);
  }, [schedule]);

  useEffect(() => {
    if (typeof window === "undefined" || !window.matchMedia) return;
    const preference = window.matchMedia("(prefers-reduced-motion: reduce)");
    const syncPreference = () => {
      reducedMotion.current = preference.matches;
      if (preference.matches) resetPointer();
    };

    syncPreference();
    preference.addEventListener?.("change", syncPreference);
    return () => preference.removeEventListener?.("change", syncPreference);
  }, [resetPointer]);

  useEffect(() => {
    if (typeof window === "undefined" || typeof document === "undefined") {
      return;
    }

    const resetWhenHidden = () => {
      if (document.visibilityState !== "visible") resetPointer();
    };

    window.addEventListener("blur", resetPointer);
    document.addEventListener("visibilitychange", resetWhenHidden);
    return () => {
      window.removeEventListener("blur", resetPointer);
      document.removeEventListener("visibilitychange", resetWhenHidden);
    };
  }, [resetPointer]);

  useEffect(
    () => () => {
      if (frame.current !== null && typeof window !== "undefined") {
        window.cancelAnimationFrame(frame.current);
      }
    },
    [],
  );

  return { ref };
}
