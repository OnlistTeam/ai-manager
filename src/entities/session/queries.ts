import {
  keepPreviousData,
  queryOptions,
  useMutation,
  useQuery,
  type UseMutationOptions,
  type UseMutationResult,
  type UseQueryResult,
} from "@tanstack/react-query";
import { sessionCacheOptions } from "@/lib/query/sessionCache";
import {
  native,
  type SessionList,
  type SessionThread,
  type ToolId,
} from "@/native";

export const sessionKeys = {
  all: ["sessions"] as const,
  list: (query: string, tool: ToolId | null) =>
    ["sessions", "list", query, tool ?? "all"] as const,
  threads: ["sessions", "thread"] as const,
  thread: (reference: string) => ["sessions", "thread", reference] as const,
};

/** While typing a search, a query's result list should be evicted soon after it stops being used. */
const SEARCH_RESULT_GC_MS = 5 * 60 * 1000;

export function sessionsQueryOptions(query: string, tool: ToolId | null) {
  return queryOptions({
    queryKey: sessionKeys.list(query, tool),
    queryFn: () => native.sessions.list(query, tool),
    ...sessionCacheOptions,
    // The full list is worth keeping resident: once it's warmed, the user
    // should see content the moment they open the page. Search results are
    // just a byproduct — keeping them around long just wastes memory.
    gcTime: query === "" ? sessionCacheOptions.gcTime : SEARCH_RESULT_GC_MS,
    // Keep the previous batch of results when the search query changes, so
    // the list doesn't flash empty before repopulating.
    placeholderData: keepPreviousData,
  });
}

export function useSessions(
  query: string,
  tool: ToolId | null,
): UseQueryResult<SessionList, Error> {
  return useQuery(sessionsQueryOptions(query, tool));
}

export function useSessionThread(
  reference: string | null,
): UseQueryResult<SessionThread, Error> {
  return useQuery({
    queryKey: sessionKeys.thread(reference ?? ""),
    queryFn: () =>
      reference === null
        ? Promise.reject(new Error("A session reference is required"))
        : native.sessions.thread(reference),
    enabled: reference !== null,
    ...sessionCacheOptions,
  });
}

export function useResumeSession(): UseMutationResult<void, Error, string> {
  return useMutation({
    mutationFn: (reference) => native.sessions.resume(reference),
  });
}

/**
 * Callbacks belong here, not on `mutate()`: per-call callbacks only fire while
 * the calling component is still mounted, and a reveal outlives its button.
 */
export function useRevealSessionFolder(
  options: Pick<UseMutationOptions<void, Error, string>, "onError"> = {},
): UseMutationResult<void, Error, string> {
  return useMutation({
    mutationFn: (reference) => native.sessions.revealFolder(reference),
    ...options,
  });
}
