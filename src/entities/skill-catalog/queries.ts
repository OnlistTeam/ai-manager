import { useQuery, type UseQueryResult } from "@tanstack/react-query";
import { native, type SkillCatalogItem, type ToolId } from "@/native";
import { skillCatalogKeys } from "./keys";

export { skillCatalogKeys } from "./keys";

export function useSkillCatalog(
  tool: ToolId | null,
  enabled: boolean,
): UseQueryResult<SkillCatalogItem[], Error> {
  return useQuery({
    queryKey: skillCatalogKeys.list(tool ?? ""),
    queryFn: () =>
      tool === null ? Promise.resolve([]) : native.skills.catalog(tool),
    enabled: enabled && tool !== null,
    staleTime: 5 * 60_000,
  });
}
