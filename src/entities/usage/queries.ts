import {
  useMutation,
  useMutationState,
  useQuery,
  useQueryClient,
  type UseQueryResult,
} from "@tanstack/react-query";
import { sessionCacheOptions } from "@/lib/query/sessionCache";
import { native, usageRefreshResultSchema, type UsageOverview } from "@/native";

export const usageKeys = {
  all: ["usage"] as const,
  overview: () => ["usage", "overview"] as const,
  refresh: () => ["usage", "refresh"] as const,
};

export function useUsageOverview(): UseQueryResult<UsageOverview, Error> {
  return useQuery({
    queryKey: usageKeys.overview(),
    queryFn: () => native.usage.overview(),
    ...sessionCacheOptions,
  });
}

export function useRefreshUsage() {
  const queryClient = useQueryClient();
  const mutationKey = usageKeys.refresh();
  const mutation = useMutation({
    mutationKey,
    mutationFn: () => native.usage.refresh(),
    onSuccess: (result) => {
      queryClient.setQueryData(usageKeys.overview(), result.overview);
    },
  });
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
  const latest = states.at(-1);
  return {
    isPending: latest?.status === "pending",
    isSuccess: latest?.status === "success",
    error: latest?.error,
    data: latest?.data,
    submittedAt: latest?.submittedAt ?? 0,
    mutate: () => {
      if (queryClient.isMutating({ mutationKey, exact: true }) === 0)
        mutation.mutate();
    },
  };
}
