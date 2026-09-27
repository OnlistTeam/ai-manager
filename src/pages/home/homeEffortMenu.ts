import type { TFunction } from "i18next";
import type { ToolModelChoice } from "@/entities/provider";

/** One stop on the effort slider. */
export interface EffortStop {
  level: string;
  label: string;
}

export interface EffortMenu {
  /** What the pill says. */
  label: string;
  /** No single level is known: none is set and the model's default is not
   * known, or the models differ. */
  muted: boolean;
  /** The levels, weakest first. */
  stops: EffortStop[];
  /** The stop in force; `null` when no single level is known. */
  index: number | null;
  /** How many of the four bars are lit; none when no single level is known. */
  bars: number;
  /** A terminal variable holds the level; the slider cannot move. */
  locked: boolean;
  /** The pill's tooltip: where a held level comes from, or each model's level. */
  title: string | null;
}

/**
 * The effort pill and its slider for one tool (ADR-0055), from the effort the
 * tool itself will use in a new session: a level nothing sets reads as the
 * model's own default, so the pill always names a real level where one is
 * known. A level already in the file that is not offered, such as Claude
 * Code's `max`, is added as the strongest stop and kept until changed.
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
  const stops: EffortStop[] = levels.map((level) => ({
    level,
    label: levelLabel(level),
  }));
  const found = stops.findIndex((stop) => stop.level === current);
  const index = found < 0 ? null : found;

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
          ? t("home.effort.unset")
          : levelLabel(current),
    muted: current === null,
    stops,
    index,
    bars:
      index === null
        ? 0
        : Math.max(1, Math.round(((index + 1) / levels.length) * 4)),
    locked: effort.kind === "terminal",
    title,
  };
}
