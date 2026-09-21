import {
  useMutation,
  useQueryClient,
  type QueryClient,
  type UseMutationResult,
} from "@tanstack/react-query";
import {
  backupKeys,
  type BackupExportOutcome,
  type BackupImportOutcome,
  type BackupList,
  type RestoreOutcome,
} from "@/entities/backup";
import { updateKeys } from "@/entities/update";
import { native } from "@/native";

function writeList(queryClient: QueryClient, list: BackupList): void {
  // The backend response is the authoritative list: creating a new backup
  // may prune the oldest few per the retention policy, and the frontend
  // has no way to guess which ones survived.
  queryClient.setQueryData(backupKeys.list(), list);
}

export function useCreateBackup(): UseMutationResult<BackupList, Error, void> {
  const queryClient = useQueryClient();

  return useMutation<BackupList, Error, void>({
    mutationFn: () => native.backup.create(),
    onSuccess: (list) => {
      writeList(queryClient, list);
    },
  });
}

export function useDeleteBackup(): UseMutationResult<
  BackupList,
  Error,
  string
> {
  const queryClient = useQueryClient();

  return useMutation<BackupList, Error, string>({
    mutationFn: (name) => native.backup.remove(name),
    onSuccess: (list) => {
      writeList(queryClient, list);
    },
  });
}

export interface RenameBackupInput {
  source: string;
  name: string;
}

export function useRenameBackup(): UseMutationResult<
  BackupList,
  Error,
  RenameBackupInput
> {
  const queryClient = useQueryClient();

  return useMutation<BackupList, Error, RenameBackupInput>({
    mutationFn: ({ source, name }) => native.backup.rename(source, name),
    onSuccess: (list) => {
      writeList(queryClient, list);
    },
  });
}

export function useRestoreBackup(): UseMutationResult<
  RestoreOutcome,
  Error,
  string
> {
  const queryClient = useQueryClient();

  return useMutation<RestoreOutcome, Error, string>({
    mutationFn: (name) => native.backup.restore(name),
    onSuccess: (outcome) => {
      // A restore swaps out the entire database: tools, services, extensions,
      // and settings could all have changed. Listing keys one by one would
      // eventually miss one, so we invalidate everything. The one exception
      // is the update check — it has nothing to do with the database, and
      // there's no reason to hit the network again on every restore.
      void queryClient.invalidateQueries({
        predicate: (query) => query.queryKey[0] !== updateKeys.all[0],
      });
      writeList(queryClient, outcome.backups);
    },
  });
}

export function useExportBackupArchive(): UseMutationResult<
  BackupExportOutcome,
  Error,
  void
> {
  return useMutation<BackupExportOutcome, Error, void>({
    mutationFn: () => native.backup.exportArchive(),
  });
}

export function useImportBackupArchive(): UseMutationResult<
  BackupImportOutcome,
  Error,
  void
> {
  const queryClient = useQueryClient();

  return useMutation<BackupImportOutcome, Error, void>({
    mutationFn: () => native.backup.importArchive(),
    onSuccess: (outcome) => {
      if (outcome.status !== "imported") return;
      void queryClient.invalidateQueries({
        predicate: (query) => query.queryKey[0] !== updateKeys.all[0],
      });
      writeList(queryClient, outcome.backups);
    },
  });
}
