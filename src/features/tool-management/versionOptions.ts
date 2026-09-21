import type { ToolVersionTag } from "@/entities/tool";

export interface TaggedVersionOption {
  version: string;
  tags: string[];
}

export interface VersionOptionGroups {
  defaultVersion: string | null;
  taggedPublished: TaggedVersionOption[];
  otherPublished: string[];
}

/**
 * The backend supplies the version recommended for ordinary automatic
 * installs and updates. Tagged and historical releases remain separate so the
 * user only enters this picker for an intentional compatibility switch.
 */
export function groupVersionOptions(
  versions: readonly string[],
  recommendedVersion: string | null | undefined,
  distTags: readonly ToolVersionTag[] = [],
): VersionOptionGroups {
  const defaultVersion =
    recommendedVersion && versions.includes(recommendedVersion)
      ? recommendedVersion
      : null;
  const tagsByVersion = new Map<string, Set<string>>();
  for (const { tag, version } of distTags) {
    if (
      tag === "latest" ||
      version === defaultVersion ||
      !versions.includes(version)
    ) {
      continue;
    }
    const tags = tagsByVersion.get(version) ?? new Set<string>();
    tags.add(tag);
    tagsByVersion.set(version, tags);
  }
  const taggedPublished = versions.flatMap((version) => {
    const tags = tagsByVersion.get(version);
    return tags
      ? [
          {
            version,
            tags: [...tags].sort((left, right) => left.localeCompare(right)),
          },
        ]
      : [];
  });
  return {
    defaultVersion,
    taggedPublished,
    otherPublished: versions.filter(
      (version) => version !== defaultVersion && !tagsByVersion.has(version),
    ),
  };
}
