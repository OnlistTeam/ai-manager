import { LoaderCircle } from "lucide-react";
import { TERMINAL_TURNTABLE } from "./terminalModel";
import { Card } from "@/shared/ui/Card";
import { SpatialScene } from "@/shared/ui/SpatialScene";
import { useSpatialPointer } from "@/shared/ui/useSpatialPointer";

export interface EnvironmentHeroSkeletonProps {
  title: string;
  label: string;
}

/**
 * Honest first-read state for the Home focal card. It mirrors the final Hero
 * footprint so an unknown environment never flashes as Attention or as an
 * unexplained empty rectangle.
 */
export function EnvironmentHeroSkeleton({
  title,
  label,
}: EnvironmentHeroSkeletonProps) {
  // The model turning toward the pointer is a property of it as an object,
  // independent of whether data has arrived. Without this line, the model on
  // the first screen would ignore the mouse for the entire loading period
  // and only suddenly start following once the real hero swaps in.
  const pointer = useSpatialPointer<HTMLDivElement>(TERMINAL_TURNTABLE);

  return (
    <Card
      ref={pointer.ref}
      padding="none"
      role="status"
      aria-label={label}
      data-model="environment"
      data-tone="neutral"
      data-spatial-stage=""
      className="environment-hero environment-hero--loading spatial-page-hero relative isolate min-h-[430px] overflow-hidden lg:min-h-[350px]"
    >
      <span aria-hidden="true" className="spatial-page-hero__wash" />
      <span aria-hidden="true" className="spatial-page-hero__lines" />

      <div className="environment-hero__layout spatial-page-hero__layout min-h-[430px] lg:min-h-[350px]">
        <SpatialScene
          icon={LoaderCircle}
          model="environment"
          turntable={TERMINAL_TURNTABLE}
          tone="neutral"
          size="hero"
          loading
          className="spatial-page-hero__scene"
        />

        <div className="spatial-page-hero__content">
          <p className="spatial-page-hero__eyebrow">{title}</p>
          <h1 className="spatial-page-hero__title">{label}</h1>
        </div>
      </div>
    </Card>
  );
}
