import type { ReactNode } from "react";
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";

/**
 * A fresh client per test case: retry is disabled so failing cases don't wait on retries.
 * `gcTime` is left at the library default — zeroing it would cause a cache entry
 * written via `setQueryData` with no active observer (e.g. a test that only
 * mounts a mutation without rendering the matching `useQuery` alongside it)
 * to be garbage-collected on the next macrotask, so reading right after the
 * write would return `undefined`.
 */
export function createTestQueryClient(): QueryClient {
  return new QueryClient({
    defaultOptions: {
      queries: { retry: false },
      mutations: { retry: false },
    },
  });
}

export function withQueryClient(client: QueryClient) {
  return function Wrapper({ children }: { children: ReactNode }) {
    return (
      <QueryClientProvider client={client}>{children}</QueryClientProvider>
    );
  };
}
