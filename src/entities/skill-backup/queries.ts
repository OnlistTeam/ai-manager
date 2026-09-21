import {
  queryOptions,
  useQuery,
  type UseQueryResult,
} from "@tanstack/react-query";
import { sessionCacheOptions } from "@/lib/query/sessionCache";
import { native, type SkillBackup } from "@/native";
import { skillBackupKeys } from "./keys";

export function skillBackupsQueryOptions(enabled = true) {
  return queryOptions({
    queryKey: skillBackupKeys.list(),
    queryFn: () => native.skills.backups(),
    enabled,
    ...sessionCacheOptions,
  });
}

export function useSkillBackups(
  enabled = true,
): UseQueryResult<SkillBackup[], Error> {
  return useQuery(skillBackupsQueryOptions(enabled));
}
