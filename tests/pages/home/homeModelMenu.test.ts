import { describe, expect, it } from "vitest";
import {
  buildModelItems,
  filterModelItems,
  type ModelMenuEndpoint,
  type ModelMenuItem,
} from "@/pages/home/homeModelMenu";

const COPY = { toolDefault: "Default", toolDefaultNote: "What it ships with" };

const RELAY: ModelMenuEndpoint = {
  provider: null,
  signIn: false,
  name: "Relay",
  detail: null,
  title: null,
  savedModel: "glm-5",
  models: ["glm-5", "glm-5-air"],
  loading: false,
};

const labels = (items: readonly ModelMenuItem[]) =>
  items.map((item) => item.label);

describe("buildModelItems", () => {
  it("starts with the default, then the model in use, the saved one and the catalogue", () => {
    const items = buildModelItems(RELAY, "glm-5-turbo", COPY);

    expect(labels(items)).toEqual([
      "Default",
      "glm-5-turbo",
      "glm-5",
      "glm-5-air",
    ]);
    expect(items[0]).toMatchObject({
      model: null,
      note: "What it ships with",
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
    expect(labels(filterModelItems(items, "ships"))).toEqual(["Default"]);
  });
});
