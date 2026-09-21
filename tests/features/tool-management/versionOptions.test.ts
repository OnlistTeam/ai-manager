import { describe, expect, it } from "vitest";
import { groupVersionOptions } from "@/features/tool-management/versionOptions";

describe("groupVersionOptions", () => {
  it("keeps npm latest as the default install target even when larger artifacts exist", () => {
    expect(
      groupVersionOptions(["1.0.11", "1.0.5", "0.1.4", "0.1.3"], "0.1.4"),
    ).toEqual({
      defaultVersion: "0.1.4",
      taggedPublished: [],
      otherPublished: ["1.0.11", "1.0.5", "0.1.3"],
    });
  });

  it("does not invent a default when the registry omits the latest tag", () => {
    expect(groupVersionOptions(["2.0.0", "1.0.0"], null)).toEqual({
      defaultVersion: null,
      taggedPublished: [],
      otherPublished: ["2.0.0", "1.0.0"],
    });
  });

  it("does not label a latest tag that is absent from the published catalog", () => {
    expect(groupVersionOptions(["2.0.0", "1.0.0"], "3.0.0")).toEqual({
      defaultVersion: null,
      taggedPublished: [],
      otherPublished: ["2.0.0", "1.0.0"],
    });
  });

  it("separates publisher-tagged channels from untagged releases", () => {
    expect(
      groupVersionOptions(["1.0.12", "1.0.11", "1.0.5"], "1.0.5", [
        { tag: "latest", version: "1.0.5" },
        { tag: "alpha", version: "1.0.12" },
        { tag: "next", version: "1.0.12" },
        { tag: "orphan", version: "9.9.9" },
      ]),
    ).toEqual({
      defaultVersion: "1.0.5",
      taggedPublished: [{ version: "1.0.12", tags: ["alpha", "next"] }],
      otherPublished: ["1.0.11"],
    });
  });
});
