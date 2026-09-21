import {
  queryOptions,
  useMutation,
  useQuery,
  useQueryClient,
  type UseMutationResult,
  type UseQueryResult,
} from "@tanstack/react-query";
import { backupKeys } from "@/entities/backup";
import { extensionKeys } from "@/entities/extension";
import { healthKeys } from "@/entities/health";
import { providerKeys } from "@/entities/provider";
import { native, type ImportOutcome, type ImportPreview } from "@/native";
import { sessionCacheOptions } from "@/lib/query/sessionCache";

export const importKeys = {
  all: ["import"] as const,
  preview: () => ["import", "preview"] as const,
};

export function importPreviewQueryOptions() {
  return queryOptions({
    queryKey: importKeys.preview(),
    queryFn: () => native.importExisting.preview(),
    ...sessionCacheOptions,
  });
}

export function useImportPreview(
  enabled = true,
): UseQueryResult<ImportPreview, Error> {
  return useQuery({
    ...importPreviewQueryOptions(),
    enabled,
  });
}

export function useRunImport(): UseMutationResult<ImportOutcome, Error, void> {
  const queryClient = useQueryClient();
  return useMutation({
    mutationFn: () => native.importExisting.run(),
    onSuccess: async () => {
      // The source never copies product settings, so that cache remains authoritative. Every
      // imported category (plus derived health) must be read again before another screen uses it.
      await Promise.all([
        queryClient.invalidateQueries({ queryKey: providerKeys.all }),
        queryClient.invalidateQueries({ queryKey: extensionKeys.all }),
        queryClient.invalidateQueries({ queryKey: backupKeys.all }),
        queryClient.invalidateQueries({ queryKey: healthKeys.all }),
      ]);
    },
  });
}
