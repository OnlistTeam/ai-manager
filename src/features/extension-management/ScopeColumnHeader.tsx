import { cn } from "@/shared/ui/cn";
import { desktopAppShortName } from "@/shared/ui/DesktopAppArtwork";
import { toolShortName } from "@/shared/ui/ToolArtwork";
import { Tooltip, TooltipContent, TooltipTrigger } from "@/shared/ui/Tooltip";
import {
  SCOPE_CELL_WIDTH_CLASS,
  SCOPE_COLUMNS_CLASS,
  ScopeGlyph,
  type ScopeToggleTarget,
} from "./ScopeToggleGroup";

export interface ScopeColumnHeaderProps {
  /** Said on the left, e.g. how many items the list has. */
  label?: string;
  targets: readonly ScopeToggleTarget[];
}

function shortName(target: ScopeToggleTarget): string {
  return target.scope.kind === "tool"
    ? toolShortName(target.scope.id)
    : desktopAppShortName(target.scope.id);
}

/**
 * Names the column of switches under it, in the same order and width, so the
 * marks on each row can be read without hovering (ADR-0062). Every switch
 * already names its app for assistive technology, so this row is visual only.
 */
export function ScopeColumnHeader({ label, targets }: ScopeColumnHeaderProps) {
  return (
    <div
      aria-hidden="true"
      className="flex min-w-0 items-end gap-x-4 px-4 pb-1.5 pt-2"
    >
      <span className="min-w-0 flex-1 truncate text-caption text-content-muted">
        {label}
      </span>
      <div className={SCOPE_COLUMNS_CLASS}>
        {targets.map((target) => (
          <Tooltip key={target.key}>
            <TooltipTrigger asChild>
              <span
                data-scope-column={target.key}
                className={cn(
                  "flex flex-col items-center gap-0.5",
                  SCOPE_CELL_WIDTH_CLASS,
                )}
              >
                <span className="relative flex h-4 w-4 items-center justify-center">
                  <ScopeGlyph scope={target.scope} />
                </span>
                {/* The label may use the gap on either side of its column
                    (a centered flex item overflows evenly), so "Claude"
                    fits over a 32px switch while the columns stay put. */}
                <span className="w-9 shrink-0 truncate text-center text-[10px] leading-3 text-content-muted">
                  {shortName(target)}
                </span>
              </span>
            </TooltipTrigger>
            <TooltipContent side="top">{target.name}</TooltipContent>
          </Tooltip>
        ))}
      </div>
    </div>
  );
}
