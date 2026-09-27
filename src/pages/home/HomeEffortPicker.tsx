import { Brain, ChevronDown, LoaderCircle } from "lucide-react";
import { useTranslation } from "react-i18next";
import type { ToolModelChoice } from "@/entities/provider";
import type { Tool } from "@/entities/tool";
import { Button } from "@/shared/ui/Button";
import { cn } from "@/shared/ui/cn";
import { OptionPicker } from "@/shared/ui/OptionPicker";
import { buildEffortMenu, TOOL_DEFAULT_EFFORT } from "./homeEffortMenu";

export interface HomeEffortPickerProps {
  tool: Tool;
  choice: ToolModelChoice | undefined;
  /** The tool's files could not be read; there is nothing to show or choose. */
  unavailable: boolean;
  busy: boolean;
  onChoose: (effort: string | null) => void;
}

/** Every effort pill shares one width; a tool without one keeps the slot. */
const PILL_CLASS = "h-7 w-32 shrink-0 rounded-full px-3 text-caption";

/**
 * How hard the tool thinks, where the tool has such a setting (ADR-0055).
 * The pill names the effort a new session will actually run at; a level
 * already in the file that is not offered is shown as it is and kept until
 * changed.
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

  const menu = buildEffortMenu(choice, tool.name, t);

  return (
    <OptionPicker
      label={t("home.effort.pickerLabel", { tool: tool.name })}
      options={menu.options}
      note={
        <>
          {menu.notes.map((line) => (
            <span key={line} className="block">
              {line}
            </span>
          ))}
        </>
      }
      filterPlaceholder={t("home.tools.filter")}
      noMatch={t("home.tools.noMatch")}
      filterAbove={Number.POSITIVE_INFINITY}
      onSelect={(id) => onChoose(id === TOOL_DEFAULT_EFFORT ? null : id)}
    >
      <Button
        variant="secondary"
        size="xs"
        disabled={busy}
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
        ) : (
          <Brain
            className="h-3.5 w-3.5 shrink-0 text-content-muted"
            aria-hidden="true"
          />
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
    </OptionPicker>
  );
}
