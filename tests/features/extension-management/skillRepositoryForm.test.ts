import { describe, expect, it } from "vitest";
import {
  parseSkillRepositoryLocation,
  skillRepositoryDraftFrom,
  validateSkillRepository,
} from "@/features/extension-management/skillRepositoryForm";

describe("skillRepositoryForm", () => {
  it.each([
    ["anthropics/skills", { owner: "anthropics", repository: "skills" }],
    [
      "https://github.com/example-org/example-skills.git",
      { owner: "example-org", repository: "example-skills" },
    ],
  ])("accepts the trusted GitHub form %s", (input, expected) => {
    expect(parseSkillRepositoryLocation(input)).toEqual(expected);
  });

  it.each([
    "http://github.com/owner/repo",
    "https://example.com/owner/repo",
    "https://github.com:443/owner/repo",
    "https://user@github.com/owner/repo",
    "https://github.com/owner/repo?ref=main",
    "https://github.com/owner/repo#readme",
    "https://github.com/owner/repo/extra",
    "owner",
    "owner/repo/extra",
    "owner with space/repo",
  ])("rejects an unsafe or malformed source: %s", (input) => {
    expect(parseSkillRepositoryLocation(input)).toBeNull();
    expect(validateSkillRepository({ location: input, branch: "" })).toEqual({
      location: "extensions.skill.repositories.validation.location",
    });
  });

  it("uses the upstream HEAD sentinel when branch detection is requested", () => {
    expect(
      skillRepositoryDraftFrom({
        location: " example-org/example-skills ",
        branch: "   ",
      }),
    ).toEqual({
      owner: "example-org",
      repository: "example-skills",
      branch: "HEAD",
      enabled: true,
    });
  });

  it("trims an explicit branch without widening the draft", () => {
    expect(
      skillRepositoryDraftFrom({
        location: "example-org/example-skills",
        branch: " release/v2 ",
      }),
    ).toEqual({
      owner: "example-org",
      repository: "example-skills",
      branch: "release/v2",
      enabled: true,
    });
  });
});
