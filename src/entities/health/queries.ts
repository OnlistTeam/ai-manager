import {
  queryOptions,
  useQuery,
  type QueryClient,
  type UseQueryResult,
} from "@tanstack/react-query";
import {
  native,
  type HealthSnapshot,
  type ProviderTestResult,
  type ToolId,
} from "@/native";
import { sessionCacheOptions } from "@/lib/query/sessionCache";
import { canonicalHealthTools, healthKeys } from "./keys";

export { canonicalHealthTools, healthKeys } from "./keys";

export type ProviderConnectivity = Partial<
  Record<ToolId, Record<string, ProviderTestResult>>
>;

const EMPTY_CONNECTIVITY: ProviderConnectivity = {};

export function healthSnapshotQueryOptions(installed: readonly ToolId[]) {
  const tools = canonicalHealthTools(installed);
  return queryOptions({
    queryKey: healthKeys.snapshot(tools),
    queryFn: () => native.health.snapshot(tools),
    ...sessionCacheOptions,
  });
}

export function useHealthSnapshot(
  installed: readonly ToolId[],
  enabled = true,
): UseQueryResult<HealthSnapshot, Error> {
  return useQuery({
    ...healthSnapshotQueryOptions(installed),
    enabled,
  });
}

/**
 * Last results from checks the user explicitly ran. This query never persists to disk and never
 * fetches: it is a process-local observable map shared by Services, Home and Settings.
 */
export function useProviderConnectivity(): UseQueryResult<
  ProviderConnectivity,
  Error
> {
  return useQuery({
    queryKey: healthKeys.connectivity(),
    queryFn: () => Promise.resolve(EMPTY_CONNECTIVITY),
    initialData: EMPTY_CONNECTIVITY,
    staleTime: Number.POSITIVE_INFINITY,
    gcTime: Number.POSITIVE_INFINITY,
  });
}

export function writeProviderConnectivity(
  queryClient: QueryClient,
  tool: ToolId,
  providerId: string,
  result: ProviderTestResult,
): void {
  queryClient.setQueryData<ProviderConnectivity>(
    healthKeys.connectivity(),
    (current = EMPTY_CONNECTIVITY) => ({
      ...current,
      [tool]: {
        ...current[tool],
        [providerId]: result,
      },
    }),
  );
}

/** Applies one operation snapshot in a single cache write. Replayed partial
 * snapshots are idempotent because provider ids overwrite their own result. */
export function writeProviderConnectivityBatch(
  queryClient: QueryClient,
  tool: ToolId,
  results: readonly ProviderTestResult[],
): void {
  if (results.length === 0) return;
  queryClient.setQueryData<ProviderConnectivity>(
    healthKeys.connectivity(),
    (current = EMPTY_CONNECTIVITY) => ({
      ...current,
      [tool]: {
        ...current[tool],
        ...Object.fromEntries(
          results.map((result) => [result.providerId, result]),
        ),
      },
    }),
  );
}

export function clearProviderConnectivity(
  queryClient: QueryClient,
  tool: ToolId,
  providerId: string,
): void {
  queryClient.setQueryData<ProviderConnectivity>(
    healthKeys.connectivity(),
    (current = EMPTY_CONNECTIVITY) => {
      const remaining = Object.fromEntries(
        Object.entries(current[tool] ?? {}).filter(
          ([storedId]) => storedId !== providerId,
        ),
      );
      const next = { ...current };
      if (Object.keys(remaining).length > 0) next[tool] = remaining;
      else delete next[tool];
      return next;
    },
  );
}

export function clearProviderConnectivityBatch(
  queryClient: QueryClient,
  tool: ToolId,
  providerIds: readonly string[],
): void {
  if (providerIds.length === 0) return;
  const cleared = new Set(providerIds);
  queryClient.setQueryData<ProviderConnectivity>(
    healthKeys.connectivity(),
    (current = EMPTY_CONNECTIVITY) => {
      const remaining = Object.fromEntries(
        Object.entries(current[tool] ?? {}).filter(
          ([storedId]) => !cleared.has(storedId),
        ),
      );
      const next = { ...current };
      if (Object.keys(remaining).length > 0) next[tool] = remaining;
      else delete next[tool];
      return next;
    },
  );
}

export function connectivityFor(
  connectivity: ProviderConnectivity,
  tool: ToolId,
  providerId: string,
): ProviderTestResult | undefined {
  return connectivity[tool]?.[providerId];
}
