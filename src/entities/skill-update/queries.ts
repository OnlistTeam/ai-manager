import {
  queryOptions,
  useQuery,
  type UseQueryResult,
} from "@tanstack/react-query";
import { sessionCacheOptions } from "@/lib/query/sessionCache";
import { native, type SkillUpdate } from "@/native";
import { skillUpdateKeys } from "./keys";

export function skillUpdatesQueryOptions(enabled = true) {
  return queryOptions({
    queryKey: skillUpdateKeys.list(),
    queryFn: () => native.skills.checkUpdates(),
    enabled,
    ...sessionCacheOptions,
  });
}

export function useSkillUpdates(
  enabled = true,
): UseQueryResult<SkillUpdate[], Error> {
  return useQuery(skillUpdatesQueryOptions(enabled));
}
