import type { TFunction } from "i18next";
import type { ToolModelChoice } from "@/entities/provider";
import type { OptionPickerOption } from "@/shared/ui/OptionPicker";

/** The option that hands the effort back to each model's own default. */
export const TOOL_DEFAULT_EFFORT = "tool-default";

export interface EffortMenu {
  /** What the pill says. */
  label: string;
  /** Nothing single is chosen here: the tool default, or levels that differ. */
  muted: boolean;
  options: OptionPickerOption[];
  /** Plain-language lines under the list, in order. */
  notes: string[];
}

/**
 * The effort pill and its list for one tool (ADR-0055), from the effort the
 * tool itself will use in a new session. A level held by a terminal variable
 * is shown and not choosable; a level the tool keeps only through its own
 * variable carries a note that it holds every session.
 */
export function buildEffortMenu(
  choice: ToolModelChoice,
  toolName: string,
  t: TFunction,
): EffortMenu {
  const levelLabel = (level: string) =>
    t(`home.effort.level.${level}`, { defaultValue: level });
  const effort = choice.effort;
  const current = "level" in effort ? effort.level : null;
  const locked = effort.kind === "terminal";
  const levels =
    current !== null && !choice.effortLevels.includes(current)
      ? [...choice.effortLevels, current]
      : choice.effortLevels;

  const notes: string[] = [];
  if (effort.kind === "terminal") {
    notes.push(
      effort.source.kind === "shellFile"
        ? t("home.effort.terminalAt", {
            variable: effort.source.variable,
            path: effort.source.path,
          })
        : t("home.effort.terminal", {
            variable:
              effort.source.kind === "environment"
                ? effort.source.variable
                : "",
          }),
    );
  } else {
    notes.push(t("home.effort.newSessions", { tool: toolName }));
  }
  if (effort.kind === "mixed") {
    const list = effort.perModel
      .map(({ model, effort: level, modelDefault }) =>
        modelDefault
          ? t("home.effort.modelDefault", {
              model,
              level: levelLabel(level),
            })
          : `${model} ${levelLabel(level)}`,
      )
      .join(" · ");
    notes.push(t("home.effort.perModel", { list }));
  }
  if (!locked && choice.variableOnlyLevels.length > 0) {
    notes.push(
      t("home.effort.variableOnly", {
        tool: toolName,
        levels: choice.variableOnlyLevels.map(levelLabel).join(" / "),
      }),
    );
  }
  if (!locked) notes.push(t("home.effort.toolDefaultNote"));

  return {
    label:
      effort.kind === "mixed"
        ? t("home.effort.mixed")
        : current === null
          ? t("home.model.toolDefault")
          : levelLabel(current),
    muted: current === null,
    options: [
      {
        id: TOOL_DEFAULT_EFFORT,
        label: t("home.model.toolDefault"),
        checked: effort.kind === "toolDefault",
        disabled: locked,
      },
      ...levels.map((level) => ({
        id: level,
        label: levelLabel(level),
        detail: levelLabel(level) === level ? null : level,
        checked: level === current,
        disabled: locked,
      })),
    ],
    notes,
  };
}
