import { useEffect } from "react";
import {
  queryOptions,
  useMutation,
  useQuery,
  useQueryClient,
  type QueryClient,
  type UseMutationResult,
  type UseQueryResult,
} from "@tanstack/react-query";
import { extensionKeys } from "@/entities/extension";
import { healthKeys } from "@/entities/health";
import { providerKeys } from "@/entities/provider";
import {
  native,
  onDeepLinkPending,
  type DeepLinkImportOutcome,
  type DeepLinkPreview,
} from "@/native";
import { sessionCacheOptions } from "@/lib/query/sessionCache";
import { deepLinkKeys } from "./keys";

export function deepLinkPendingQueryOptions() {
  return queryOptions({
    queryKey: deepLinkKeys.pending(),
    queryFn: () => native.deepLink.pending(),
    ...sessionCacheOptions,
  });
}

export function useDeepLinkPending(): UseQueryResult<DeepLinkPreview[], Error> {
  return useQuery(deepLinkPendingQueryOptions());
}

/**
 * The queue lives in native memory and can change without the renderer asking,
 * so this is the one subscriber that refreshes it. It mirrors
 * `useOperationEvents`: subscribe once, near the root.
 */
export function useDeepLinkEvents(): void {
  const queryClient = useQueryClient();

  useEffect(() => {
    let disposed = false;
    const subscription = onDeepLinkPending(() => {
      void queryClient.invalidateQueries({ queryKey: deepLinkKeys.pending() });
    });

    return () => {
      disposed = true;
      void subscription.then((unlisten) => {
        if (disposed) unlisten();
      });
    };
  }, [queryClient]);
}

export function useSubmitPastedDeepLink(): UseMutationResult<
  DeepLinkPreview,
  Error,
  string
> {
  const queryClient = useQueryClient();
  return useMutation({
    mutationFn: (link: string) => native.deepLink.submitPasted(link),
    onSuccess: async () => {
      await queryClient.invalidateQueries({ queryKey: deepLinkKeys.pending() });
    },
  });
}

export function useDismissDeepLink(): UseMutationResult<void, Error, string> {
  const queryClient = useQueryClient();
  return useMutation({
    mutationFn: (pending: string) => native.deepLink.dismiss(pending),
    onSuccess: async () => {
      await queryClient.invalidateQueries({ queryKey: deepLinkKeys.pending() });
    },
    // A link that is already gone is the outcome the caller wanted; drop the
    // stale row instead of leaving a dead entry with an error next to it.
    onError: async () => {
      await queryClient.invalidateQueries({ queryKey: deepLinkKeys.pending() });
    },
  });
}

export function useConfirmDeepLink(): UseMutationResult<
  DeepLinkImportOutcome,
  Error,
  string
> {
  const queryClient = useQueryClient();
  return useMutation({
    mutationFn: (pending: string) => native.deepLink.confirm(pending),
    onSuccess: (outcome) => invalidateForOutcome(queryClient, outcome),
  });
}

/**
 * An import writes through the same entries the ordinary screens use, so the
 * same caches go stale. Skills and MCP servers finish in the background and
 * `useOperationEvents` refreshes them again when their task ends; this first
 * pass is what makes the change visible immediately.
 */
async function invalidateForOutcome(
  queryClient: QueryClient,
  outcome: DeepLinkImportOutcome,
): Promise<void> {
  const invalidations = [
    queryClient.invalidateQueries({ queryKey: deepLinkKeys.pending() }),
  ];
  if (outcome.resource === "provider") {
    invalidations.push(
      queryClient.invalidateQueries({ queryKey: providerKeys.all }),
      queryClient.invalidateQueries({ queryKey: healthKeys.all }),
    );
  } else {
    invalidations.push(
      queryClient.invalidateQueries({ queryKey: extensionKeys.all }),
    );
  }
  await Promise.all(invalidations);
}
