import { useId, type ComponentType } from "react";
import { ArrowUpRight } from "lucide-react";
import { useTranslation } from "react-i18next";
import { cn } from "@/shared/ui/cn";
import { FOCUS_RING } from "@/shared/ui/focusRing";

export interface QuickAction {
  id: string;
  labelKey: string;
  descriptionKey: string;
  icon: ComponentType<{ className?: string }>;
  onSelect?: () => void;
  disabled?: boolean;
  /** The reason for disabling; it must be written out — just graying it out without telling the user why is the most frustrating thing. */
  hintKey?: string;
}

export interface QuickActionsProps {
  actions: readonly QuickAction[];
}

/** The whole-card entry point from Spec §29; when disabled, the reason must go into both the visible copy and the aria description. */
export function QuickActions({ actions }: QuickActionsProps) {
  const { t } = useTranslation();
  const idPrefix = useId();

  return (
    <div className="grid min-w-0 gap-3 sm:grid-cols-2 lg:grid-cols-3">
      {actions.map((action) => {
        const Icon = action.icon;
        const titleId = `${idPrefix}-${action.id}-title`;
        const descriptionId = `${idPrefix}-${action.id}-description`;
        const hintId = action.hintKey
          ? `${idPrefix}-${action.id}-hint`
          : undefined;
        return (
          <button
            key={action.id}
            type="button"
            disabled={action.disabled}
            onClick={action.onSelect}
            aria-labelledby={titleId}
            aria-describedby={
              [descriptionId, hintId].filter(Boolean).join(" ") || undefined
            }
            className={cn(
              "quick-action-card group relative flex min-h-[144px] w-full flex-col items-start overflow-hidden rounded-2xl border border-hairline bg-layer-1 p-5 text-left shadow-md",
              "transition-[transform,box-shadow,border-color,background-color] duration-fast ease-standard",
              action.disabled
                ? "cursor-not-allowed border-hairline bg-layer-1 shadow-none"
                : "motion-safe:hover:-translate-y-0.5 hover:border-brand/30 hover:bg-layer-2 hover:shadow-md",
              FOCUS_RING,
            )}
          >
            <span className="flex w-full items-start justify-between gap-3">
              <span
                className={cn(
                  "quick-action-card__icon flex h-11 w-11 items-center justify-center rounded-xl border",
                  action.disabled
                    ? "border-hairline bg-layer-1 text-content-muted"
                    : "border-brand/10 bg-brand/10 text-brand",
                )}
              >
                <Icon className="h-5 w-5" aria-hidden="true" />
              </span>
              {!action.disabled ? (
                <span className="quick-action-card__arrow flex h-8 w-8 items-center justify-center rounded-full border border-hairline bg-layer-1 text-content-muted">
                  <ArrowUpRight
                    data-slot="quick-action-arrow"
                    className="h-4 w-4 transition-transform duration-fast ease-standard group-hover:text-brand motion-safe:group-hover:-translate-y-0.5 motion-safe:group-hover:translate-x-0.5"
                    aria-hidden="true"
                  />
                </span>
              ) : null}
            </span>
            <span id={titleId} className="mt-4 text-heading text-content">
              {t(action.labelKey)}
            </span>
            <span
              id={descriptionId}
              className="mt-1.5 text-caption leading-[18px] text-content-muted"
            >
              {t(action.descriptionKey)}
            </span>
            {action.hintKey ? (
              <span
                id={hintId}
                className="mt-2.5 inline-flex rounded-lg border border-hairline bg-layer-1 px-2.5 py-1 text-caption text-content-muted"
              >
                {t(action.hintKey)}
              </span>
            ) : null}
          </button>
        );
      })}
    </div>
  );
}
