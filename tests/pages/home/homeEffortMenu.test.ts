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
  buildEffortMenu({ ...choice, effort }, "Claude Code", i18n.t.bind(i18n));

const checked = (effort: ToolModelChoice["effort"]) =>
  menu(effort)
    .options.filter((option) => option.checked)
    .map((option) => option.id);

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

  it("names the tool default and checks it", () => {
    const shown = menu({ kind: "toolDefault" });
    expect(shown.label).toBe("Tool default");
    expect(shown.muted).toBe(true);
    expect(checked({ kind: "toolDefault" })).toEqual(["tool-default"]);
    expect(shown.notes).toContain(en.home.effort.toolDefaultNote);
  });

  it("names a level the settings give and checks it", () => {
    const shown = menu({ kind: "level", level: "xhigh" });
    expect(shown.label).toBe("Extra high");
    expect(shown.muted).toBe(false);
    expect(checked({ kind: "level", level: "xhigh" })).toEqual(["xhigh"]);
  });

  it("says the models differ, checks nothing, and lists each level", () => {
    const effort = {
      kind: "mixed" as const,
      perModel: [
        { model: "claude-opus-5-5", effort: "medium", modelDefault: true },
        { model: "claude-opus-5", effort: "high", modelDefault: false },
      ],
    };
    const shown = menu(effort);
    expect(shown.label).toBe("Per model");
    expect(shown.muted).toBe(true);
    expect(checked(effort)).toEqual([]);
    expect(shown.notes).toContain(
      "Each model runs at its own level now: claude-opus-5-5 Medium (its default) · claude-opus-5 High. Choosing one here sets it for every model.",
    );
  });

  it("names a level the variable fixes and explains that it holds", () => {
    const shown = menu({ kind: "fixed", level: "max" });
    expect(shown.label).toBe("Max");
    expect(checked({ kind: "fixed", level: "max" })).toEqual(["max"]);
    expect(shown.notes).toContain(
      "Max holds for every session, and /effort in Claude Code cannot change it until you choose another level here.",
    );
  });

  it("keeps an unknown level in the list and leaves out the variable note where there is none", () => {
    const shown = menu(
      { kind: "level", level: "minimal" },
      {
        ...CLAUDE,
        tool: "codex",
        effortLevels: ["low", "medium", "high", "xhigh"],
        variableOnlyLevels: [],
      },
    );
    expect(shown.options.map((option) => option.id)).toEqual([
      "tool-default",
      "low",
      "medium",
      "high",
      "xhigh",
      "minimal",
    ]);
    expect(shown.notes.some((note) => note.includes("/effort"))).toBe(false);
  });

  it("offers nothing when a terminal variable holds the level", () => {
    const shown = menu({
      kind: "terminal",
      level: "high",
      source: { kind: "environment", variable: "CLAUDE_CODE_EFFORT_LEVEL" },
    });
    expect(shown.label).toBe("High");
    expect(shown.options.every((option) => option.disabled)).toBe(true);
    expect(shown.notes).toEqual([
      "Set by the terminal variable CLAUDE_CODE_EFFORT_LEVEL; it cannot be changed here.",
    ]);
  });
});
