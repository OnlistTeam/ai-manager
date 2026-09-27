import {
  useMutation,
  useMutationState,
  useQuery,
  useQueryClient,
  type QueryClient,
  type UseQueryResult,
} from "@tanstack/react-query";
import { useCallback } from "react";
import { sessionCacheOptions } from "@/lib/query/sessionCache";
import { native, usageRefreshResultSchema, type UsageOverview } from "@/native";

export const usageKeys = {
  all: ["usage"] as const,
  overview: () => ["usage", "overview"] as const,
  refresh: () => ["usage", "refresh"] as const,
};

/**
 * An automatic sync is skipped when one started this recently. Unchanged
 * session files are skipped by the native cursor, so the gap only keeps tab
 * switches and quick window round-trips from rescanning back to back.
 */
export const USAGE_AUTO_SYNC_GAP_MS = 30_000;

export function useUsageOverview(): UseQueryResult<UsageOverview, Error> {
  return useQuery({
    queryKey: usageKeys.overview(),
    queryFn: () => native.usage.overview(),
    ...sessionCacheOptions,
  });
}

function lastSyncStartedAt(queryClient: QueryClient): number {
  return queryClient
    .getMutationCache()
    .findAll({ mutationKey: usageKeys.refresh(), exact: true })
    .reduce((latest, sync) => Math.max(latest, sync.state.submittedAt), 0);
}

export function useRefreshUsage() {
  const queryClient = useQueryClient();
  const mutationKey = usageKeys.refresh();
  const mutation = useMutation({
    mutationKey,
    mutationFn: () => native.usage.refresh(),
    onSuccess: async (result) => {
      // A read that began before the scan wrote must not land on top of it.
      await queryClient.cancelQueries({ queryKey: usageKeys.overview() });
      queryClient.setQueryData(usageKeys.overview(), result.overview);
    },
  });
  const start = mutation.mutate;
  // The native scan outlives the page. Observe its shared mutation, not just
  // this mount's observer, so navigating back cannot enqueue a second scan.
  const states = useMutationState({
    filters: { mutationKey, exact: true },
    select: ({ state }) => ({
      status: state.status,
      error: state.error,
      submittedAt: state.submittedAt,
      data: usageRefreshResultSchema.safeParse(state.data).data,
    }),
  });
  const mutate = useCallback(() => {
    const running = queryClient.isMutating({
      mutationKey: usageKeys.refresh(),
      exact: true,
    });
    if (running === 0) start();
  }, [queryClient, start]);
  const syncIfDue = useCallback(() => {
    if (Date.now() - lastSyncStartedAt(queryClient) >= USAGE_AUTO_SYNC_GAP_MS)
      mutate();
  }, [queryClient, mutate]);
  const latest = states.at(-1);
  return {
    isPending: latest?.status === "pending",
    isSuccess: latest?.status === "success",
    error: latest?.error,
    data: latest?.data,
    submittedAt: latest?.submittedAt ?? 0,
    mutate,
    syncIfDue,
  };
}
