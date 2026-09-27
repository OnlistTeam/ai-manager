import { describe, expect, it } from "vitest";
import {
  buildModelMenu,
  type ModelMenuEndpoint,
} from "@/pages/home/homeModelMenu";

const COPY = { toolDefault: "Tool default", loading: "Loading models…" };

function endpoint(overrides: Partial<ModelMenuEndpoint>): ModelMenuEndpoint {
  return {
    providerId: "relay",
    name: "Relay",
    detail: null,
    inUse: false,
    savedModel: null,
    models: [],
    loading: false,
    ...overrides,
  };
}

describe("buildModelMenu", () => {
  it("offers the tool default only under the endpoint in use, and checks the model in force", () => {
    const menu = buildModelMenu(
      [
        endpoint({
          providerId: "official",
          name: "Official",
          inUse: true,
          savedModel: "opus",
          models: ["fable", "opus"],
        }),
        endpoint({ savedModel: "glm-5", models: ["glm-5", "glm-5-air"] }),
      ],
      "opus",
      true,
      COPY,
    );

    const [inUse, other] = menu.groups;
    expect(inUse?.header).toMatchObject({ label: "Official", checked: true });
    expect(inUse?.options.map((option) => option.label)).toEqual([
      "Tool default",
      "opus",
      "fable",
    ]);
    expect(inUse?.options.find((option) => option.checked)?.label).toBe("opus");
    // Another endpoint names the model saved with it, and lists it first.
    expect(other?.header).toMatchObject({ label: "Relay", detail: "glm-5" });
    expect(other?.options.map((option) => option.label)).toEqual([
      "glm-5",
      "glm-5-air",
    ]);
    expect(menu.choices.get(other?.options[1]?.id ?? "")).toEqual({
      kind: "model",
      providerId: "relay",
      model: "glm-5-air",
    });
    expect(menu.choices.get(inUse?.options[0]?.id ?? "")).toEqual({
      kind: "model",
      providerId: "official",
      model: null,
    });
    expect(menu.choices.get(other?.header.id ?? "")).toEqual({
      kind: "endpoint",
      providerId: "relay",
    });
  });

  it("lists what a tool without a saved entry uses as current and not switchable", () => {
    const menu = buildModelMenu(
      [
        endpoint({
          providerId: null,
          name: "relay.example.test",
          detail: "Terminal variable",
          inUse: true,
          models: [],
          loading: true,
        }),
      ],
      null,
      true,
      COPY,
    );
    const [group] = menu.groups;
    expect(group?.header).toMatchObject({
      checked: true,
      disabled: true,
      detail: "Terminal variable",
    });
    expect(menu.choices.has(group?.header.id ?? "")).toBe(false);
    expect(group?.options.map((option) => option.label)).toEqual([
      "Tool default",
    ]);
    expect(group?.note).toBe("Loading models…");
    expect(menu.choices.get(group?.options[0]?.id ?? "")).toEqual({
      kind: "model",
      providerId: null,
      model: null,
    });
  });

  it("lists only the endpoints where the model is not chosen here", () => {
    const menu = buildModelMenu(
      [endpoint({ inUse: true, models: ["glm-5"] })],
      "glm-5",
      false,
      COPY,
    );
    expect(menu.groups[0]?.options).toEqual([]);
  });
});
