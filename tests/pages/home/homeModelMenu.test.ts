import { describe, expect, it } from "vitest";
import {
  buildModelSections,
  viewModelSections,
  type ModelMenuEndpoint,
  type ModelMenuItem,
} from "@/pages/home/homeModelMenu";

const COPY = { toolDefault: "Default", toolDefaultNote: "What it ships with" };

function endpoint(overrides: Partial<ModelMenuEndpoint>): ModelMenuEndpoint {
  return {
    key: "relay",
    providerId: "relay",
    provider: null,
    signIn: false,
    name: "Relay",
    detail: null,
    title: null,
    inUse: false,
    savedModel: null,
    models: [],
    loading: false,
    ...overrides,
  };
}

const OFFICIAL = endpoint({
  key: "official",
  providerId: "official",
  name: "Official",
  inUse: true,
  savedModel: "opus",
  models: ["fable", "opus"],
});
const RELAY = endpoint({ savedModel: "glm-5", models: ["glm-5", "glm-5-air"] });

const labels = (items: readonly ModelMenuItem[]) =>
  items.map((item) => item.label);

describe("buildModelSections", () => {
  it("puts the endpoint in use first and starts each with the default", () => {
    const sections = buildModelSections([RELAY, OFFICIAL], "opus", COPY);

    expect(sections.map((section) => section.endpoint.key)).toEqual([
      "official",
      "relay",
    ]);
    const [inUse, other] = sections;
    expect(labels(inUse.items)).toEqual(["Default", "opus", "fable"]);
    expect(labels(other.items)).toEqual(["Default", "glm-5", "glm-5-air"]);
    // The note is said once, on the first section's default.
    expect(inUse.items[0].note).toBe("What it ships with");
    expect(other.items[0].note).toBeNull();
    expect(
      sections.flatMap((section) =>
        section.items.filter((item) => item.checked).map((item) => item.id),
      ),
    ).toEqual(["official\u001fopus"]);
  });

  it("checks the default when the tool names no model", () => {
    const [inUse] = buildModelSections([OFFICIAL], null, COPY);
    expect(inUse.items[0]).toMatchObject({ model: null, checked: true });
  });
});

describe("viewModelSections", () => {
  const sections = buildModelSections([OFFICIAL, RELAY], "opus", COPY);
  const none = () => false;

  it("narrows to one endpoint", () => {
    const shown = viewModelSections(sections, { endpoint: "relay" }, "", none);
    expect(shown.map((section) => section.endpoint.key)).toEqual(["relay"]);
  });

  it("keeps only the starred models", () => {
    const starred = (item: ModelMenuItem) => item.model === "glm-5-air";
    const shown = viewModelSections(sections, "favorites", "", starred);
    expect(shown.map((section) => labels(section.items))).toEqual([
      ["glm-5-air"],
    ]);
  });

  it("filters by name and drops an endpoint left empty", () => {
    const shown = viewModelSections(sections, "all", "GLM-5-a", none);
    expect(shown.map((section) => labels(section.items))).toEqual([
      ["glm-5-air"],
    ]);
  });
});
