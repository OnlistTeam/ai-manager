import type { TFunction } from "i18next";
import type { ToolModelChoice } from "@/entities/provider";

/** One stop on the effort slider; `null` hands the effort back to the tool. */
export interface EffortStop {
  level: string | null;
  label: string;
}

export interface EffortMenu {
  /** What the pill says. */
  label: string;
  /** Nothing single is chosen here: the tool default, or levels that differ. */
  muted: boolean;
  /** The tool default first, then the levels weakest first. */
  stops: EffortStop[];
  /** The stop in force; `null` when the models differ. */
  index: number | null;
  /** How many of the four bars are lit; none for the default or a mix. */
  bars: number;
  /** A terminal variable holds the level; the slider cannot move. */
  locked: boolean;
  /** The pill's tooltip: where a held level comes from, or each model's level. */
  title: string | null;
}

/**
 * The effort pill and its slider for one tool (ADR-0055), from the effort the
 * tool itself will use in a new session. A level already in the file that is
 * not offered, such as Claude Code's `max`, is added as the strongest stop and
 * kept until changed.
 */
export function buildEffortMenu(
  choice: ToolModelChoice,
  t: TFunction,
): EffortMenu {
  const levelLabel = (level: string) =>
    t(`home.effort.level.${level}`, { defaultValue: level });
  const effort = choice.effort;
  const current = "level" in effort ? effort.level : null;
  const levels =
    current !== null && !choice.effortLevels.includes(current)
      ? [...choice.effortLevels, current]
      : choice.effortLevels;
  const stops: EffortStop[] = [
    { level: null, label: t("home.model.toolDefault") },
    ...levels.map((level) => ({ level, label: levelLabel(level) })),
  ];
  const index =
    effort.kind === "mixed"
      ? null
      : Math.max(
          0,
          stops.findIndex((stop) => stop.level === current),
        );

  let title: string | null = null;
  if (effort.kind === "terminal") {
    title =
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
          });
  } else if (effort.kind === "mixed") {
    title = effort.perModel
      .map(({ model, effort: level }) => `${model} ${levelLabel(level)}`)
      .join("\n");
  }

  return {
    label:
      effort.kind === "mixed"
        ? t("home.effort.mixed")
        : current === null
          ? t("home.model.toolDefault")
          : levelLabel(current),
    muted: current === null,
    stops,
    index,
    bars: index ? Math.max(1, Math.round((index / levels.length) * 4)) : 0,
    locked: effort.kind === "terminal",
    title,
  };
}
