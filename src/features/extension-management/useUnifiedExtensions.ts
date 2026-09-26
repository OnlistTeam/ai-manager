import { useQueries } from "@tanstack/react-query";
import {
  extensionsQueryOptions,
  type Extension,
  type ExtensionKind,
} from "@/entities/extension";
import type { ExtensionScopeOption } from "./extensionTabs";
import {
  unifiedExtensionRows,
  type UnifiedExtensionRow,
} from "./unifiedExtensionRows";

export interface UnifiedExtensions {
  rows: UnifiedExtensionRow[];
  /** Every app's list has arrived at least once. */
  dataAvailable: boolean;
  initiallyLoading: boolean;
  /** Some app's list has never been read, so no complete row can be shown. */
  unavailable: boolean;
  /** Rows are shown from the last good read, but a refresh failed. */
  refreshFailed: boolean;
  isFetching: boolean;
  isError: boolean;
  refetch: () => void;
}

/**
 * The per-app lists the page already reads, combined into one row per item.
 * Each list stays its own Query (the same cache entries toggles write back
 * to), so no aggregate native command is needed and a toggle in one app
 * never refetches the others.
 */
export function useUnifiedExtensions(
  kind: ExtensionKind,
  targets: readonly ExtensionScopeOption[],
): UnifiedExtensions {
  return useQueries({
    queries: targets.map((target) =>
      extensionsQueryOptions(target.scope, kind),
    ),
    combine: (results) => {
      const lists = results.flatMap((result) =>
        result.data === undefined ? [] : [result.data as Extension[]],
      );
      const dataAvailable =
        results.length > 0 && lists.length === results.length;
      const unavailable = results.some(
        (result) => result.isFetched && result.data === undefined,
      );
      const isError = results.some((result) => result.isError);
      return {
        rows: dataAvailable ? unifiedExtensionRows(lists) : [],
        dataAvailable,
        initiallyLoading: results.length > 0 && !dataAvailable && !unavailable,
        unavailable,
        refreshFailed: dataAvailable && isError,
        isFetching: results.some((result) => result.isFetching),
        isError,
        refetch: () => {
          for (const result of results) void result.refetch();
        },
      };
    },
  });
}
