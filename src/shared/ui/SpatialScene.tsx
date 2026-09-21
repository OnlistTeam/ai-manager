import type { CSSProperties, ReactNode } from "react";
import type { LucideIcon } from "lucide-react";
import { cn } from "./cn";
import {
  NEUTRAL_TURNTABLE_X,
  turntableFrameAt,
  type SpatialTurntable,
} from "./turntableFrame";

export type SpatialSceneModel =
  | "environment"
  | "tools"
  | "services"
  | "extensions"
  | "settings";

export type SpatialSceneTone =
  | "brand"
  | "success"
  | "warning"
  | "danger"
  | "neutral";

interface SpatialSceneBaseProps {
  model: SpatialSceneModel;
  tone?: SpatialSceneTone;
  size?: "compact" | "standard" | "hero";
  loading?: boolean;
  /** Original transparent product artwork; the code-native model is fallback. */
  artwork?: string;
  /**
   * Turntable sequence: one sprite sheet plus its grid. Each frame is
   * actually rendered from the corresponding yaw angle, so as it follows the
   * pointer the side walls, top face, and base top genuinely show and hide
   * with the angle — something a single texture can never do no matter how
   * you rotate it, since it has no side faces to reveal.
   */
  turntable?: SpatialTurntable & { sheet: string };
  className?: string;
}

export type SpatialSceneProps = SpatialSceneBaseProps &
  ({ icon: LucideIcon; glyph?: never } | { icon?: never; glyph: ReactNode });

/**
 * Grid dimensions, plus this grid's neutral starting pose.
 *
 * The starting pose has to come from here, because only this function knows
 * its own grid. The CSS fallback value is 0%, i.e. the first frame of the
 * sequence — one extreme of yaw, not dead center. Any caller that never wires
 * up the pointer (skeleton screens, failure states) would otherwise end up
 * with the model twisted off to one side. Once the on-stage pointer writes
 * `--spatial-frame-*`, the inherited value overrides this default.
 */
function turntableStyle(turntable: SpatialTurntable): CSSProperties {
  const neutral = turntableFrameAt(turntable, NEUTRAL_TURNTABLE_X);
  return {
    "--turntable-cols": turntable.cols,
    "--turntable-rows": turntable.rows,
    "--turntable-neutral-x": neutral.x,
    "--turntable-neutral-y": neutral.y,
    "--turntable-neutral-next-x": neutral.nextX,
    "--turntable-neutral-next-y": neutral.nextY,
    "--turntable-neutral-blend": neutral.blend,
  } as CSSProperties;
}

/**
 * A code-native 3D visual shared by every product page. It is decorative:
 * page headings and status copy remain the accessible source of truth.
 */
export function SpatialScene({
  icon: Icon,
  glyph,
  model,
  tone = "brand",
  size = "standard",
  loading = false,
  artwork,
  turntable,
  className,
}: SpatialSceneProps) {
  return (
    <div
      aria-hidden="true"
      // The pointer hook turns the model towards the cursor from anywhere in
      // the window, and measures the angle from this element's centre.
      data-spatial-origin=""
      data-model={model}
      data-size={size}
      data-tone={tone}
      data-loading={loading || undefined}
      data-artwork={artwork || turntable ? "true" : undefined}
      data-turntable={turntable ? "true" : undefined}
      className={cn("spatial-scene", className)}
    >
      <span className="spatial-scene__ambient" />
      <span className="spatial-scene__floor" />
      <span className="spatial-scene__grid" />

      <span className="spatial-scene__tilt">
        <span className="spatial-scene__object">
          {turntable ? (
            <span className="spatial-scene__artwork-shell">
              <span
                className="spatial-scene__turntable"
                style={turntableStyle(turntable)}
              >
                <img
                  src={turntable.sheet}
                  alt=""
                  draggable={false}
                  className="spatial-scene__turntable-strip"
                />
                {/* A second copy of the same sheet, pinned to the next frame. Cross-fading
                    the two makes the discrete sequence read as continuous rotation under
                    the pointer instead of jumping frame by frame. Since the src is
                    identical, the browser only decodes it once. */}
                <img
                  src={turntable.sheet}
                  alt=""
                  draggable={false}
                  className="spatial-scene__turntable-strip spatial-scene__turntable-strip--next"
                />
              </span>
              <span className="spatial-scene__artwork-glint" />
            </span>
          ) : artwork ? (
            <span className="spatial-scene__artwork-shell">
              <img
                src={artwork}
                alt=""
                draggable={false}
                className="spatial-scene__artwork"
              />
              <span className="spatial-scene__artwork-glint" />
            </span>
          ) : (
            <>
              <span className="spatial-scene__orbit spatial-scene__orbit--one">
                <span className="spatial-scene__satellite" />
              </span>
              <span className="spatial-scene__orbit spatial-scene__orbit--two">
                <span className="spatial-scene__satellite" />
              </span>
              <span className="spatial-scene__orbit spatial-scene__orbit--three">
                <span className="spatial-scene__satellite" />
              </span>

              <span className="spatial-scene__motif">
                <span className="spatial-scene__motif-part spatial-scene__motif-part--one" />
                <span className="spatial-scene__motif-part spatial-scene__motif-part--two" />
                <span className="spatial-scene__motif-part spatial-scene__motif-part--three" />
                <span className="spatial-scene__motif-part spatial-scene__motif-part--four" />
              </span>

              <span className="spatial-scene__sculpture">
                <span className="spatial-scene__sculpture-piece spatial-scene__sculpture-piece--one" />
                <span className="spatial-scene__sculpture-piece spatial-scene__sculpture-piece--two" />
                <span className="spatial-scene__sculpture-piece spatial-scene__sculpture-piece--three" />
                <span className="spatial-scene__sculpture-piece spatial-scene__sculpture-piece--four" />
              </span>

              <span className="spatial-scene__core">
                <span className="spatial-scene__glint" />
                <span className="spatial-scene__meridian" />
                <span className="spatial-scene__equator" />
                <span className="spatial-scene__glyph">
                  {glyph ?? (Icon ? <Icon /> : null)}
                </span>
              </span>

              <span className="spatial-scene__shard spatial-scene__shard--one" />
              <span className="spatial-scene__shard spatial-scene__shard--two" />
              <span className="spatial-scene__shard spatial-scene__shard--three" />
            </>
          )}
        </span>
      </span>
    </div>
  );
}
