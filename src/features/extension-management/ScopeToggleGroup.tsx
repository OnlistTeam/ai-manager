import { LoaderCircle, Monitor } from "lucide-react";
import { useTranslation } from "react-i18next";
import type { ExtensionScope } from "@/entities/extension";
import { cn } from "@/shared/ui/cn";
import { DesktopAppGlyph } from "@/shared/ui/DesktopAppArtwork";
import { FOCUS_RING } from "@/shared/ui/focusRing";
import { ToolGlyph } from "@/shared/ui/ToolArtwork";
import { Tooltip, TooltipContent, TooltipTrigger } from "@/shared/ui/Tooltip";

export interface ScopeToggleTarget {
  key: string;
  name: string;
  scope: ExtensionScope;
}

export interface ScopeToggleGroupProps {
  /** The item's name, so every switch has its own accessible name. */
  itemName: string;
  targets: readonly ScopeToggleTarget[];
  isOn: (target: ScopeToggleTarget) => boolean;
  pendingKey?: string | null;
  /** The switch whose last write failed; it is renamed as a retry. */
  failedKey?: string | null;
  retryLabel?: string | null;
  disabled: boolean;
  onToggle: (target: ScopeToggleTarget, enabled: boolean) => void;
}

/**
 * A desktop app can share its maker's mark with a CLI tool (Claude Desktop and
 * Claude Code), so desktop apps carry a small screen badge.
 */
function ScopeGlyph({ scope }: { scope: ExtensionScope }) {
  if (scope.kind === "tool") return <ToolGlyph toolId={scope.id} />;
  return (
    <>
      <DesktopAppGlyph appId={scope.id} />
      <Monitor
        className="absolute bottom-0.5 right-0.5 h-2.5 w-2.5 text-content-muted"
        aria-hidden="true"
      />
    </>
  );
}

/**
 * One small switch per app, in the row itself, so turning an item on for a
 * second app is one click instead of a trip to another tab. On is a filled,
 * outlined tile and off is a faded grey mark, so the state never rests on
 * colour alone; the name and state are in the tooltip and the accessible name.
 */
export function ScopeToggleGroup({
  itemName,
  targets,
  isOn,
  pendingKey = null,
  failedKey = null,
  retryLabel = null,
  disabled,
  onToggle,
}: ScopeToggleGroupProps) {
  const { t } = useTranslation();

  return (
    <div
      role="group"
      aria-label={t("extensions.list.appsNamed", { name: itemName })}
      className="ml-auto flex shrink-0 items-center gap-1"
    >
      {targets.map((target) => {
        const on = isOn(target);
        const pending = pendingKey === target.key;
        const stateLabel = t(
          on ? "extensions.list.stateOn" : "extensions.list.stateOff",
          { tool: target.name },
        );

        return (
          <Tooltip key={target.key}>
            <TooltipTrigger asChild>
              <button
                type="button"
                aria-label={
                  failedKey === target.key && retryLabel
                    ? retryLabel
                    : t("extensions.list.useIn", {
                        name: itemName,
                        tool: target.name,
                      })
                }
                aria-pressed={on}
                aria-busy={pending || undefined}
                disabled={disabled}
                onClick={() => onToggle(target, !on)}
                className={cn(
                  "relative flex h-8 w-8 items-center justify-center rounded-lg border transition-[opacity,background-color,border-color] duration-fast ease-standard",
                  "enabled:hover:opacity-100 enabled:hover:grayscale-0 disabled:cursor-not-allowed",
                  on
                    ? "border-brand/40 bg-brand/10"
                    : "border-transparent opacity-40 grayscale",
                  FOCUS_RING,
                )}
              >
                {pending ? (
                  <LoaderCircle
                    className="h-4 w-4 text-brand motion-safe:animate-spin"
                    aria-hidden="true"
                  />
                ) : (
                  <ScopeGlyph scope={target.scope} />
                )}
              </button>
            </TooltipTrigger>
            <TooltipContent side="bottom">{stateLabel}</TooltipContent>
          </Tooltip>
        );
      })}
    </div>
  );
}
