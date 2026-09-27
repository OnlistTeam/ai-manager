import i18n from "i18next";
import { beforeAll, describe, expect, it } from "vitest";
import type { ToolModelChoice } from "@/entities/provider";
import en from "@/i18n/locales/en.json";
import { buildEffortMenu } from "@/pages/home/homeEffortMenu";

const CLAUDE: ToolModelChoice = {
  tool: "claude-code",
  model: null,
  effort: { kind: "toolDefault" },
  effortLevels: ["low", "medium", "high", "xhigh", "max"],
  variableOnlyLevels: ["max"],
  officialModels: ["opus"],
};

const menu = (effort: ToolModelChoice["effort"], choice = CLAUDE) =>
  buildEffortMenu({ ...choice, effort }, i18n.t.bind(i18n));

describe("buildEffortMenu", () => {
  beforeAll(async () => {
    i18n.addResourceBundle(
      "en",
      "translation",
      { home: { effort: en.home.effort, model: en.home.model } },
      true,
      true,
    );
    await i18n.changeLanguage("en");
  });

  it("puts the default first, then the levels weakest first", () => {
    const shown = menu({ kind: "toolDefault" });
    expect(shown.stops.map((stop) => stop.label)).toEqual([
      "Default",
      "Low",
      "Medium",
      "High",
      "Extra high",
      "Max",
    ]);
    expect(shown.label).toBe("Default");
    expect(shown.muted).toBe(true);
    expect(shown.index).toBe(0);
    expect(shown.bars).toBe(0);
    expect(shown.title).toBeNull();
  });

  it("names a level the settings give and lights bars up to it", () => {
    const shown = menu({ kind: "level", level: "xhigh" });
    expect(shown.label).toBe("Extra high");
    expect(shown.muted).toBe(false);
    expect(shown.index).toBe(4);
    expect(shown.bars).toBe(3);
    expect(menu({ kind: "level", level: "low" }).bars).toBe(1);
    expect(menu({ kind: "fixed", level: "max" }).bars).toBe(4);
  });

  it("says the models differ, stands on no stop, and names each in the tooltip", () => {
    const shown = menu({
      kind: "mixed",
      perModel: [
        { model: "claude-opus-5-5", effort: "medium", modelDefault: true },
        { model: "claude-opus-5", effort: "high", modelDefault: false },
      ],
    });
    expect(shown.label).toBe("Per model");
    expect(shown.index).toBeNull();
    expect(shown.bars).toBe(0);
    expect(shown.title).toBe("claude-opus-5-5 Medium\nclaude-opus-5 High");
  });

  it("locks a level a terminal variable holds and says where it is set", () => {
    const shown = menu({
      kind: "terminal",
      level: "high",
      source: {
        kind: "shellFile",
        variable: "CLAUDE_CODE_EFFORT_LEVEL",
        path: "~/.zshrc:12",
      },
    });
    expect(shown.locked).toBe(true);
    expect(shown.label).toBe("High");
    expect(shown.title).toBe(
      "Set by the terminal variable CLAUDE_CODE_EFFORT_LEVEL in ~/.zshrc:12; it cannot be changed here.",
    );
  });

  it("adds a level in the file that is not offered as the strongest stop", () => {
    const codex = {
      ...CLAUDE,
      tool: "codex" as const,
      effortLevels: ["low", "medium", "high", "xhigh"],
      variableOnlyLevels: [],
    };
    const shown = menu({ kind: "level", level: "minimal" }, codex);
    expect(shown.stops.map((stop) => stop.level)).toEqual([
      null,
      "low",
      "medium",
      "high",
      "xhigh",
      "minimal",
    ]);
    expect(shown.index).toBe(5);
    expect(shown.label).toBe("Minimal");
  });
});
