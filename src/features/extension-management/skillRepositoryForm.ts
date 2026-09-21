import type { SkillRepositoryDraft } from "@/entities/skill-repository";

export interface SkillRepositoryValues {
  location: string;
  branch: string;
}

export interface SkillRepositoryFormErrors {
  location?: string;
}

export const EMPTY_SKILL_REPOSITORY_VALUES: SkillRepositoryValues = {
  location: "",
  branch: "",
};

const OWNER_PATTERN = /^[A-Za-z0-9-]{1,39}$/;
const REPOSITORY_PATTERN = /^[A-Za-z0-9._-]{1,100}$/;

export function parseSkillRepositoryLocation(
  input: string,
): { owner: string; repository: string } | null {
  const value = input.trim();
  if (value === "") return null;

  let path = value;
  if (value.includes("://")) {
    // Keep the accepted spelling intentionally narrow. URL normalizes an
    // explicit default port away, so check the authority shape first.
    if (!/^https:\/\/github\.com\//i.test(value)) return null;
    let url: URL;
    try {
      url = new URL(value);
    } catch {
      return null;
    }
    if (
      url.protocol !== "https:" ||
      url.hostname.toLocaleLowerCase() !== "github.com" ||
      url.port !== "" ||
      url.username !== "" ||
      url.password !== "" ||
      url.search !== "" ||
      url.hash !== ""
    ) {
      return null;
    }
    path = url.pathname;
  }

  const segments = path
    .replace(/^\/+|\/+$/g, "")
    .split("/")
    .filter(Boolean);
  if (segments.length !== 2) return null;
  const owner = segments[0] ?? "";
  const repository = (segments[1] ?? "").replace(/\.git$/i, "");
  if (
    !OWNER_PATTERN.test(owner) ||
    !REPOSITORY_PATTERN.test(repository) ||
    repository === "." ||
    repository === ".."
  ) {
    return null;
  }
  return { owner, repository };
}

export function validateSkillRepository(
  values: SkillRepositoryValues,
): SkillRepositoryFormErrors {
  return parseSkillRepositoryLocation(values.location)
    ? {}
    : { location: "extensions.skill.repositories.validation.location" };
}

export function skillRepositoryDraftFrom(
  values: SkillRepositoryValues,
): SkillRepositoryDraft {
  const parsed = parseSkillRepositoryLocation(values.location);
  if (!parsed) throw new Error("repository form must be validated first");
  return {
    ...parsed,
    branch: values.branch.trim() || "HEAD",
    enabled: true,
  };
}
