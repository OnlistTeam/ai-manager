import { describe, expect, it } from "vitest";
import {
  buildModelItems,
  describeModel,
  filterModelItems,
  modelPillLabel,
  type ModelMenuEndpoint,
  type ModelMenuItem,
} from "@/pages/home/homeModelMenu";

const COPY = { toolDefault: "Default", toolDefaultNote: "Let it choose" };

const RELAY: ModelMenuEndpoint = {
  provider: null,
  signIn: false,
  name: "Relay",
  detail: null,
  title: null,
  models: ["glm-5", "glm-5-air"],
  loading: false,
};

const labels = (items: readonly ModelMenuItem[]) =>
  items.map((item) => item.label);

describe("buildModelItems", () => {
  it("starts with the default, then the model in use and the catalogue", () => {
    const items = buildModelItems(RELAY, "glm-5-turbo", COPY);

    expect(labels(items)).toEqual([
      "Default",
      "glm-5-turbo",
      "glm-5",
      "glm-5-air",
    ]);
    expect(items[0]).toMatchObject({
      model: null,
      note: "Let it choose",
      checked: false,
    });
    expect(items.filter((item) => item.checked).map((item) => item.id)).toEqual(
      ["glm-5-turbo"],
    );
  });

  it("checks the default when the tool names no model", () => {
    const [first, ...rest] = buildModelItems(RELAY, null, COPY);
    expect(first).toMatchObject({ model: null, checked: true });
    expect(labels(rest)).toEqual(["glm-5", "glm-5-air"]);
  });
});

describe("filterModelItems", () => {
  const items = buildModelItems(RELAY, null, COPY);

  it("keeps every row without a filter", () => {
    expect(filterModelItems(items, "  ")).toEqual(items);
  });

  it("matches the name or the note, ignoring case", () => {
    expect(labels(filterModelItems(items, "GLM-5-a"))).toEqual(["glm-5-air"]);
    expect(labels(filterModelItems(items, "choose"))).toEqual(["Default"]);
  });
});

// ADR-0059: a model reads as Claude Code reads it, so one model looks the same
// whichever way an endpoint spells it; the id itself is never rewritten.
describe("describeModel", () => {
  it.each([
    ["gpt-5.6-sol", "gpt-5.6-sol", null, "gpt-5.6-sol"],
    ["openai/gpt-5.6-sol", "gpt-5.6-sol", "openai", "gpt-5.6-sol"],
    [
      "anthropic/openai/gpt-5.6-sol[1m]",
      "gpt-5.6-sol",
      "openai · 1M",
      "gpt-5.6-sol · 1M",
    ],
    [
      "anthropic/claude-opus-5-5",
      "claude-opus-5-5",
      "anthropic",
      "claude-opus-5-5",
    ],
    ["claude-opus-5-5[1M]", "claude-opus-5-5", "1M", "claude-opus-5-5 · 1M"],
    ["weird/", "weird/", null, "weird/"],
  ])("%s", (id, name, note, pill) => {
    expect(describeModel(id)).toMatchObject({ name, note });
    expect(modelPillLabel(id)).toBe(pill);
  });

  it("labels catalogue rows by name and keeps the id they write", () => {
    const gateway: ModelMenuEndpoint = {
      ...RELAY,
      models: ["anthropic/openai/gpt-5.6-sol[1m]", "anthropic/claude-opus-5-5"],
    };
    const [, sol, opus] = buildModelItems(gateway, null, COPY);
    expect(sol).toMatchObject({
      model: "anthropic/openai/gpt-5.6-sol[1m]",
      label: "gpt-5.6-sol",
      note: "openai · 1M",
    });
    expect(opus).toMatchObject({
      model: "anthropic/claude-opus-5-5",
      label: "claude-opus-5-5",
    });
    // A pasted full id still finds its row.
    expect(
      filterModelItems(
        buildModelItems(gateway, null, COPY),
        "anthropic/openai",
      ).map((item) => item.model),
    ).toEqual(["anthropic/openai/gpt-5.6-sol[1m]"]);
  });
});
