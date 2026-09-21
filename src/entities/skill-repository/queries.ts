import {
  queryOptions,
  useQuery,
  type UseQueryResult,
} from "@tanstack/react-query";
import { sessionCacheOptions } from "@/lib/query/sessionCache";
import { native, type SkillRepository } from "@/native";
import { skillRepositoryKeys } from "./keys";

export function skillRepositoriesQueryOptions(enabled = true) {
  return queryOptions({
    queryKey: skillRepositoryKeys.list(),
    queryFn: () => native.skills.repositories(),
    enabled,
    ...sessionCacheOptions,
  });
}

export function useSkillRepositories(
  enabled = true,
): UseQueryResult<SkillRepository[], Error> {
  return useQuery(skillRepositoriesQueryOptions(enabled));
}
