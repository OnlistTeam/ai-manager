import * as Popover from "@radix-ui/react-popover";
import { ChevronDown, LoaderCircle, Lock } from "lucide-react";
import { useEffect, useRef, useState } from "react";
import { useTranslation } from "react-i18next";
import type { ToolModelChoice } from "@/entities/provider";
import type { Tool } from "@/entities/tool";
import { Button } from "@/shared/ui/Button";
import { cn } from "@/shared/ui/cn";
import { buildEffortMenu, type EffortMenu } from "./homeEffortMenu";

export interface HomeEffortPickerProps {
  tool: Tool;
  choice: ToolModelChoice | undefined;
  /** The tool's files could not be read; there is nothing to show or choose. */
  unavailable: boolean;
  busy: boolean;
  onChoose: (effort: string) => void;
}

/** Every effort pill shares one width; a tool without one keeps the slot. */
const PILL_CLASS = "h-7 w-32 shrink-0 rounded-full px-3 text-caption";

/** A level is written once the slider has rested this long. */
const SETTLE_MS = 300;

/** Four rising bars, lit up to the level: none when no single level is known. */
function EffortBars({ lit }: { lit: number }) {
  return (
    <svg
      viewBox="0 0 16 16"
      aria-hidden="true"
      className="h-3.5 w-3.5 shrink-0 text-content"
    >
      {[4, 7, 10, 13].map((height, bar) => (
        <rect
          key={height}
          x={1.25 + bar * 3.6}
          y={14.5 - height}
          width={2.6}
          height={height}
          rx={1}
          fill="currentColor"
          opacity={bar < lit ? 1 : 0.28}
        />
      ))}
    </svg>
  );
}

function EffortSlider({
  menu,
  onChoose,
}: {
  menu: EffortMenu;
  onChoose: (effort: string) => void;
}) {
  const { t } = useTranslation();
  const [position, setPosition] = useState(menu.index ?? 0);
  const [moved, setMoved] = useState(false);
  const range = useRef<HTMLInputElement>(null);
  const pending = useRef<{
    stop: number;
    timer: ReturnType<typeof setTimeout>;
  }>();
  const last = menu.stops.length - 1;
  // With no single level known the thumb stands on no stop until one is
  // picked, rather than on the weakest as if that were in force.
  const unplaced = menu.index === null && !moved;
  const fill = unplaced || last <= 0 ? 0 : (position / last) * 100;

  const write = (stop: number) => {
    pending.current = undefined;
    if (stop !== menu.index) onChoose(menu.stops[stop].level);
  };
  const flush = useRef(write);
  flush.current = write;
  const settle = (stop: number) => {
    if (pending.current) clearTimeout(pending.current.timer);
    pending.current = {
      stop,
      timer: setTimeout(() => flush.current(stop), SETTLE_MS),
    };
  };
  // Dragging only shows the level. The native `change` event, which fires
  // when the thumb is let go or a key moves it, schedules the write, and
  // quick key presses settle into one; closing the slider still writes the
  // stop it was left on.
  useEffect(() => {
    const input = range.current;
    const onChange = () => {
      if (input) settle(Number(input.value));
    };
    input?.addEventListener("change", onChange);
    return () => {
      input?.removeEventListener("change", onChange);
      const left = pending.current;
      if (!left) return;
      clearTimeout(left.timer);
      flush.current(left.stop);
    };
  }, []);

  const move = (next: number) => {
    setPosition(next);
    setMoved(true);
  };

  return (
    <>
      <div className="flex items-baseline justify-between gap-3">
        <span className="text-caption text-content-muted">
          {t("home.effort.title")}
        </span>
        <span className="truncate text-body font-medium text-content">
          {moved ? menu.stops[position].label : menu.label}
        </span>
      </div>
      <div className="relative mt-3 h-5">
        <div className="absolute inset-x-2 top-1/2 h-1 -translate-y-1/2 rounded-full bg-layer-2">
          <div
            className="h-full rounded-full bg-brand"
            style={{ width: `${fill}%` }}
          />
          {menu.stops.map((stop, stopIndex) => (
            <i
              key={stop.level}
              className={cn(
                "absolute top-1/2 h-1.5 w-1.5 -translate-x-1/2 -translate-y-1/2 rounded-full",
                !unplaced && stopIndex <= position
                  ? "bg-brand"
                  : "bg-hairline-strong",
              )}
              style={{ left: `${last > 0 ? (stopIndex / last) * 100 : 0}%` }}
            />
          ))}
        </div>
        <input
          ref={range}
          type="range"
          min={0}
          max={last}
          step={1}
          value={position}
          disabled={menu.locked}
          aria-label={t("home.effort.title")}
          aria-valuetext={unplaced ? menu.label : menu.stops[position].label}
          onChange={(event) => move(Number(event.target.value))}
          // A click on the stop the unplaced thumb sits at changes no value,
          // so no `change` fires; it still picks that stop.
          onPointerUp={(event) => {
            if (!unplaced) return;
            const stop = Number(event.currentTarget.value);
            move(stop);
            settle(stop);
          }}
          className={cn(
            "absolute inset-0 h-5 w-full cursor-pointer appearance-none bg-transparent disabled:cursor-not-allowed",
            unplaced && "[&::-webkit-slider-thumb]:opacity-0",
            "[&::-webkit-slider-thumb]:h-4 [&::-webkit-slider-thumb]:w-4 [&::-webkit-slider-thumb]:appearance-none [&::-webkit-slider-thumb]:rounded-full [&::-webkit-slider-thumb]:border [&::-webkit-slider-thumb]:border-brand [&::-webkit-slider-thumb]:bg-white [&::-webkit-slider-thumb]:shadow-md",
            "focus-visible:[&::-webkit-slider-thumb]:ring-2 focus-visible:[&::-webkit-slider-thumb]:ring-brand/40",
          )}
        />
      </div>
      <div className="mt-1 flex justify-between text-caption text-content-muted">
        <span>{menu.stops[0].label}</span>
        <span>{menu.stops[last].label}</span>
      </div>
      {menu.locked && menu.title ? (
        <p className="mt-2 text-caption text-content-muted">{menu.title}</p>
      ) : null}
    </>
  );
}

/**
 * How hard the tool thinks, where the tool has such a setting (ADR-0055):
 * a pill with four bars and the level in force, opening a slider from the
 * weakest level to the strongest. The stop the slider is let go on is
 * written, for sessions started after it, and the slider stays open so
 * neighbouring levels can be tried in turn.
 */
export function HomeEffortPicker({
  tool,
  choice,
  unavailable,
  busy,
  onChoose,
}: HomeEffortPickerProps) {
  const { t } = useTranslation();

  if (!tool.capabilities.canChooseEffort) {
    return <span aria-hidden="true" className={cn(PILL_CLASS, "invisible")} />;
  }
  if (choice === undefined && unavailable) {
    return (
      <span
        title={t("home.effort.unavailable")}
        className={cn(
          PILL_CLASS,
          "flex items-center truncate border border-dashed border-hairline text-content-muted",
        )}
      >
        {t("home.effort.unavailable")}
      </span>
    );
  }
  if (choice === undefined) {
    return (
      <span
        aria-hidden="true"
        className={cn(PILL_CLASS, "bg-layer-2 motion-safe:animate-pulse")}
      />
    );
  }

  const menu = buildEffortMenu(choice, t);

  return (
    <Popover.Root>
      <Popover.Trigger asChild>
        <Button
          variant="secondary"
          size="xs"
          title={menu.title ?? undefined}
          aria-label={t("home.effort.pickNamed", {
            tool: tool.name,
            current: menu.label,
          })}
          className={cn(PILL_CLASS, "justify-between")}
        >
          {busy ? (
            <LoaderCircle
              className="h-3.5 w-3.5 shrink-0 motion-safe:animate-spin"
              aria-hidden="true"
            />
          ) : menu.locked ? (
            <Lock
              className="h-3.5 w-3.5 shrink-0 text-content-muted"
              aria-hidden="true"
            />
          ) : (
            <EffortBars lit={menu.bars} />
          )}
          <span
            className={cn(
              "min-w-0 flex-1 truncate text-left",
              menu.muted ? "text-content-muted" : "text-content",
            )}
          >
            {menu.label}
          </span>
          <ChevronDown
            className="h-3.5 w-3.5 shrink-0 text-content-muted"
            aria-hidden="true"
          />
        </Button>
      </Popover.Trigger>
      <Popover.Portal>
        <Popover.Content
          align="end"
          sideOffset={6}
          collisionPadding={16}
          aria-label={t("home.effort.pickerLabel", { tool: tool.name })}
          className="app-floating-menu z-[70] w-60 rounded-lg border p-3 outline-none animate-ds-overlay-in"
        >
          <EffortSlider menu={menu} onChoose={onChoose} />
        </Popover.Content>
      </Popover.Portal>
    </Popover.Root>
  );
}
