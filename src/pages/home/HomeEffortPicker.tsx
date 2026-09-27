import { Brain, ChevronDown, LoaderCircle } from "lucide-react";
import { useTranslation } from "react-i18next";
import type { ToolModelChoice } from "@/entities/provider";
import type { Tool } from "@/entities/tool";
import { Button } from "@/shared/ui/Button";
import { cn } from "@/shared/ui/cn";
import { OptionPicker } from "@/shared/ui/OptionPicker";

export interface HomeEffortPickerProps {
  tool: Tool;
  choice: ToolModelChoice | undefined;
  /** The tool's files could not be read; there is nothing to show or choose. */
  unavailable: boolean;
  busy: boolean;
  onChoose: (effort: string | null) => void;
}

/** Every effort pill shares one width; a tool without one keeps the slot. */
const PILL_CLASS = "h-7 w-28 shrink-0 rounded-full px-3 text-caption";

const TOOL_DEFAULT = "tool-default";

/**
 * How hard the tool thinks, where the tool has such a setting (ADR-0055).
 * The levels are the ones the tool's own setting accepts; a level already in
 * the file that is not among them is shown as it is and kept until changed.
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

  const levelLabel = (level: string) =>
    t(`home.effort.level.${level}`, { defaultValue: level });
  const current = choice.effort;
  const levels =
    current !== null && !choice.effortLevels.includes(current)
      ? [...choice.effortLevels, current]
      : choice.effortLevels;
  const label =
    current === null ? t("home.model.toolDefault") : levelLabel(current);
  const overrides = choice.effortOverrides
    .map(({ model, effort }) => `${model} ${levelLabel(effort)}`)
    .join(" · ");

  return (
    <OptionPicker
      label={t("home.effort.pickerLabel", { tool: tool.name })}
      options={[
        {
          id: TOOL_DEFAULT,
          label: t("home.model.toolDefault"),
          checked: current === null,
        },
        ...levels.map((level) => ({
          id: level,
          label: levelLabel(level),
          detail: levelLabel(level) === level ? null : level,
          checked: level === current,
        })),
      ]}
      note={
        <>
          <span className="block">
            {t("home.effort.newSessions", { tool: tool.name })}
          </span>
          {overrides ? (
            <span className="block">
              {t("home.effort.overrides", { tool: tool.name, list: overrides })}
            </span>
          ) : null}
        </>
      }
      filterPlaceholder={t("home.tools.filter")}
      noMatch={t("home.tools.noMatch")}
      filterAbove={Number.POSITIVE_INFINITY}
      onSelect={(id) => onChoose(id === TOOL_DEFAULT ? null : id)}
    >
      <Button
        variant="secondary"
        size="xs"
        disabled={busy}
        aria-label={t("home.effort.pickNamed", {
          tool: tool.name,
          current: label,
        })}
        className={cn(PILL_CLASS, "justify-between")}
      >
        {busy ? (
          <LoaderCircle
            className="h-3.5 w-3.5 shrink-0 motion-safe:animate-spin"
            aria-hidden="true"
          />
        ) : (
          <Brain
            className="h-3.5 w-3.5 shrink-0 text-content-muted"
            aria-hidden="true"
          />
        )}
        <span
          className={cn(
            "min-w-0 flex-1 truncate text-left",
            current === null ? "text-content-muted" : "text-content",
          )}
        >
          {label}
        </span>
        <ChevronDown
          className="h-3.5 w-3.5 shrink-0 text-content-muted"
          aria-hidden="true"
        />
      </Button>
    </OptionPicker>
  );
}
