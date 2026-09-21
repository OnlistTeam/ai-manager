import { useId, type ReactNode } from "react";
import type { LucideIcon } from "lucide-react";
import { Card } from "./Card";
import { cn } from "./cn";
import {
  SpatialScene,
  type SpatialSceneModel,
  type SpatialSceneTone,
} from "./SpatialScene";
import { useSpatialPointer } from "./useSpatialPointer";

export interface SpatialPageHeaderProps {
  title: string;
  description?: string;
  icon: LucideIcon;
  model: SpatialSceneModel;
  artwork?: string;
  tone?: SpatialSceneTone;
  eyebrow?: ReactNode;
  action?: ReactNode;
  loading?: boolean;
  as?: "h1" | "h2";
  className?: string;
}

/**
 * The page-level visual stage: one strong focal object, generous whitespace,
 * restrained glass, and a consistent place for primary page actions.
 */
export function SpatialPageHeader({
  title,
  description,
  icon,
  model,
  artwork,
  tone = "brand",
  eyebrow,
  action,
  loading = false,
  as: Heading = "h1",
  className,
}: SpatialPageHeaderProps) {
  const headingId = useId();
  const pointer = useSpatialPointer<HTMLDivElement>();

  return (
    <Card
      ref={pointer.ref}
      padding="none"
      aria-labelledby={headingId}
      aria-busy={loading || undefined}
      data-model={model}
      data-tone={tone}
      data-spatial-stage=""
      className={cn("spatial-page-hero", className)}
    >
      <span aria-hidden="true" className="spatial-page-hero__wash" />
      <span aria-hidden="true" className="spatial-page-hero__lines" />
      <div className="spatial-page-hero__layout">
        <SpatialScene
          icon={icon}
          model={model}
          artwork={artwork}
          tone={tone}
          loading={loading}
          className="spatial-page-hero__scene"
        />
        <div className="spatial-page-hero__content">
          {eyebrow ? (
            <div className="spatial-page-hero__eyebrow">{eyebrow}</div>
          ) : null}
          <Heading id={headingId} className="spatial-page-hero__title">
            {title}
          </Heading>
          {description ? (
            <p className="spatial-page-hero__description">{description}</p>
          ) : null}
          {action ? (
            <div className="spatial-page-hero__action">{action}</div>
          ) : null}
        </div>
      </div>
    </Card>
  );
}
