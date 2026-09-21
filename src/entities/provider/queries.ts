import {
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
  type Provider,
  type ProviderConnectionProfile,
  type ProviderEditProfile,
  type ProviderRuntimeContext,
  type ProviderRuntimeResourceOpenOutcome,
  type ToolId,
} from "@/native";
import { providerKeys } from "./keys";

export { providerKeys } from "./keys";

/**
 * `all` is a prefix key; each tool's provider list gets its own sub-key, so
 * switching tools never overwrites another. After a write, the caller writes
 * back directly via `setQueryData` — no extra round trip needed.
 */
/**
 * Spec §22: the provider list is server state, so it only goes through Query.
 * `tool` being null means no tool is selected yet (the tool list itself is
 * still loading), so no request is sent.
 */
export function providersQueryOptions(tool: ToolId | null) {
  return queryOptions({
    queryKey: providerKeys.list(tool ?? ""),
    queryFn: () =>
      tool === null ? Promise.resolve([]) : native.providers.list(tool),
    enabled: tool !== null,
    ...sessionCacheOptions,
  });
}

export function useProviders(
  tool: ToolId | null,
): UseQueryResult<Provider[], Error> {
  return useQuery(providersQueryOptions(tool));
}

/** Templates are owned by the backend compatibility layer; the frontend doesn't duplicate endpoints or default models per ToolId. */
export function providerConnectionProfileQueryOptions(tool: ToolId | null) {
  return queryOptions({
    queryKey: providerKeys.connectionProfile(tool ?? ""),
    queryFn: () =>
      tool === null
        ? Promise.reject(new Error("A tool is required"))
        : native.providers.connectionProfile(tool),
    enabled: tool !== null,
    ...sessionCacheOptions,
  });
}

export function useProviderConnectionProfile(
  tool: ToolId | null,
): UseQueryResult<ProviderConnectionProfile, Error> {
  return useQuery(providerConnectionProfileQueryOptions(tool));
}

export function providerRuntimeContextQueryOptions(tool: ToolId | null) {
  return queryOptions({
    queryKey: providerKeys.runtimeContext(tool ?? ""),
    queryFn: () =>
      tool === null
        ? Promise.reject(new Error("A tool is required"))
        : native.providers.runtimeContext(tool),
    enabled: tool !== null,
    ...sessionCacheOptions,
  });
}

export function useProviderRuntimeContext(
  tool: ToolId | null,
): UseQueryResult<ProviderRuntimeContext, Error> {
  return useQuery(providerRuntimeContextQueryOptions(tool));
}

export interface OpenProviderRuntimeResourceVariables {
  tool: ToolId;
  resource: string;
}

/**
 * Callbacks belong here, not on `mutate()`: per-call callbacks only fire while
 * the calling component is still mounted, and opening a file outlives a page.
 */
export function useOpenProviderRuntimeResource(
  options: Pick<
    UseMutationOptions<
      ProviderRuntimeResourceOpenOutcome,
      Error,
      OpenProviderRuntimeResourceVariables
    >,
    "onSuccess" | "onError"
  > = {},
): UseMutationResult<
  ProviderRuntimeResourceOpenOutcome,
  Error,
  OpenProviderRuntimeResourceVariables
> {
  return useMutation({
    mutationFn: ({ tool, resource }) =>
      native.providers.openRuntimeResource(tool, resource),
    ...options,
  });
}

/** Reads only the safe, redacted detail needed by the edit dialog. */
export function useProviderEditProfile(
  tool: ToolId | null,
  provider: string | null,
): UseQueryResult<ProviderEditProfile, Error> {
  return useQuery({
    queryKey: providerKeys.editProfile(tool ?? "", provider ?? ""),
    queryFn: () =>
      tool === null || provider === null
        ? Promise.reject(new Error("A tool and service are required"))
        : native.providers.editProfile(tool, provider),
    enabled: tool !== null && provider !== null,
    staleTime: 5_000,
  });
}
