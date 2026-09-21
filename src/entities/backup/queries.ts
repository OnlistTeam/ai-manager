import {
  queryOptions,
  useMutation,
  useQuery,
  useQueryClient,
  type UseMutationResult,
  type UseQueryResult,
} from "@tanstack/react-query";
import { sessionCacheOptions } from "@/lib/query/sessionCache";
import { native, type BackupList, type BackupSchedule } from "@/native";

export const backupKeys = {
  all: ["backups"] as const,
  list: () => ["backups", "list"] as const,
  schedule: () => ["backups", "schedule"] as const,
};

/**
 * Spec §22: the backup list is server state, so it only goes through Query.
 * All three write operations get the refreshed full list back from the backend;
 * the caller writes it back directly via `setQueryData` (see `features/backup`).
 */
export function backupsQueryOptions() {
  return queryOptions({
    queryKey: backupKeys.list(),
    queryFn: () => native.backup.list(),
    ...sessionCacheOptions,
  });
}

export function useBackups(): UseQueryResult<BackupList, Error> {
  return useQuery(backupsQueryOptions());
}

export function backupScheduleQueryOptions() {
  return queryOptions({
    queryKey: backupKeys.schedule(),
    queryFn: () => native.backupSchedule.get(),
    ...sessionCacheOptions,
  });
}

export function useBackupSchedule(): UseQueryResult<BackupSchedule, Error> {
  return useQuery(backupScheduleQueryOptions());
}

export function useSaveBackupSchedule(): UseMutationResult<
  BackupSchedule,
  Error,
  BackupSchedule
> {
  const queryClient = useQueryClient();
  return useMutation({
    mutationFn: (schedule) => native.backupSchedule.save(schedule),
    onSuccess: (saved) => {
      queryClient.setQueryData(backupKeys.schedule(), saved);
    },
  });
}
