import {
  queryOptions,
  useQuery,
  type UseQueryResult,
} from "@tanstack/react-query";
import { sessionCacheOptions } from "@/lib/query/sessionCache";
import {
  native,
  type Extension,
  type ExtensionKind,
  type ExtensionScope,
  type LocalExtensionInventory,
  extensionScopeKey,
} from "@/native";
import { extensionKeys } from "./keys";

export { extensionKeys } from "./keys";

/**
 * `all` is a prefix key; each (tool, kind) combination gets its own sub-key,
 * so switching tabs or tools never overwrites another. After a write, the
 * caller writes back directly via `setQueryData` — no extra round trip needed.
 */
export function localExtensionInventoryQueryOptions() {
  return queryOptions({
    queryKey: extensionKeys.localInventory(),
    queryFn: () => native.extensions.localInventory(),
    ...sessionCacheOptions,
  });
}

export function useLocalExtensionInventory(): UseQueryResult<
  LocalExtensionInventory,
  Error
> {
  return useQuery(localExtensionInventoryQueryOptions());
}

export function extensionsQueryOptions(
  scope: ExtensionScope | null,
  kind: ExtensionKind,
  supported = true,
) {
  return queryOptions({
    queryKey: extensionKeys.list(
      scope === null ? "" : extensionScopeKey(scope),
      kind,
    ),
    queryFn: () =>
      scope === null
        ? Promise.resolve([])
        : native.extensions.list(scope, kind),
    enabled: scope !== null && supported,
    ...sessionCacheOptions,
  });
}

/**
 * Spec §22: the extension list is server state, so it only goes through Query.
 * `scope` being null means no tool or desktop app is selected yet, so no
 * request is sent.
 */
export function useExtensions(
  scope: ExtensionScope | null,
  kind: ExtensionKind,
  supported = true,
): UseQueryResult<Extension[], Error> {
  return useQuery(extensionsQueryOptions(scope, kind, supported));
}
