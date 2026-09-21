import {
  useQuery,
  useQueryClient,
  type UseQueryResult,
} from "@tanstack/react-query";
import { native, type Operation } from "@/native";
import { operationKeys } from "./keys";
import { reconcileOperations } from "./operationCache";

export { operationKeys } from "./keys";

/**
 * Fetch the baseline only once on mount; after that it's driven by
 * `operation://changed` (`useOperationEvents` writes the cache). Polling
 * this would only burn IPC for nothing.
 *
 * fetch and the event bridge are two independent async paths: before fetch
 * lands, a terminal event may already have written to the cache. So queryFn
 * doesn't `setQueryData` the fetch result wholesale — it reconciles the
 * current cache against the fetch result entry by entry (see
 * `reconcileOperations`), avoiding a stale fetch snapshot knocking back a
 * terminal state that's already in the cache.
 */
export function useOperations(): UseQueryResult<Operation[], Error> {
  const queryClient = useQueryClient();
  return useQuery({
    queryKey: operationKeys.list(),
    queryFn: async () => {
      const fetched = await native.operations.list();
      const cached = queryClient.getQueryData<Operation[]>(
        operationKeys.list(),
      );
      return reconcileOperations(fetched, cached);
    },
    staleTime: Number.POSITIVE_INFINITY,
    refetchOnWindowFocus: false,
  });
}
