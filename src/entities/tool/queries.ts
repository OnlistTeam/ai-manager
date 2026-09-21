import { useCallback, useMemo } from "react";
import {
  queryOptions,
  useQuery,
  type UseQueryResult,
} from "@tanstack/react-query";
import {
  native,
  type Tool,
  type ToolId,
  type ToolUpdatePreview,
  type ToolUninstallPreview,
  type ToolVersionCatalog,
} from "@/native";
import { sessionCacheOptions } from "@/lib/query/sessionCache";
import { toolKeys } from "./keys";

export { toolKeys } from "./keys";

/**
 * `all` is a prefix key: after any action that changes install state,
 * `invalidateQueries({ queryKey: toolKeys.all })` alone is enough to
 * invalidate the whole subtree.
 */
export function toolsQueryOptions() {
  return queryOptions({
    queryKey: toolKeys.list(),
    queryFn: () => native.tools.list(),
    ...sessionCacheOptions,
  });
}

/** Spec §22: the tool list is server state, so it only goes through Query — never copy it into useState. */
export function useTools(): UseQueryResult<Tool[], Error> {
  return useQuery(toolsQueryOptions());
}

/**
 * The latest version is the only field in this list that needs the network,
 * so it's a separate query: the local list shows up first, and this one
 * fills in the gap in the background. It's fetched once per session (a few
 * KB) and never retries on failure — not knowing means not knowing,
 * `latestVersion` stays null, and nobody is allowed to read that as
 * "already up to date".
 */
export function toolVersionCheckQueryOptions() {
  return queryOptions({
    queryKey: toolKeys.versionCheck(),
    queryFn: () => native.tools.checkVersions(),
    ...sessionCacheOptions,
    retry: false,
  });
}

export function useToolVersionCheck(): UseQueryResult<Tool[], Error> {
  return useQuery(toolVersionCheckQueryOptions());
}

/** Local list plus the version info fetched over the network — this is what the page reads. */
export interface ToolInventory {
  data: Tool[] | undefined;
  isPending: boolean;
  isFetched: boolean;
  isFetching: boolean;
  isSuccess: boolean;
  isError: boolean;
  dataUpdatedAt: number;
  refetch: () => Promise<unknown>;
  /**
   * The latest-version check is running in the background. This is not a
   * state that should block the user: buttons stay usable as normal, and
   * nothing that decides "can the user act" should look at this flag.
   */
  checkingVersions: boolean;
}

/**
 * A version-check record's local version must match the current list's;
 * otherwise it's describing a different install (one that just got
 * upgraded or just got installed), and this field is better left empty —
 * giving a stale answer as if it were fresh is worse than not answering
 * at all.
 */
function withCheckedVersions(
  local: readonly Tool[],
  checked: readonly Tool[],
): Tool[] {
  const byId = new Map(checked.map((tool) => [tool.id, tool]));
  return local.map((tool) => {
    const remote = byId.get(tool.id);
    if (remote === undefined || remote.version !== tool.version) return tool;
    return {
      ...tool,
      latestVersion: remote.latestVersion,
      status: remote.status,
    };
  });
}

/** Uses the local list as the skeleton, overlaying the network-fetched version info onto it by id. */
export function useToolInventory(): ToolInventory {
  const tools = useTools();
  const check = useToolVersionCheck();
  const checked = check.data;
  const data = useMemo(() => {
    if (tools.data === undefined) return undefined;
    return checked ? withCheckedVersions(tools.data, checked) : tools.data;
  }, [checked, tools.data]);

  const toolsRefetch = tools.refetch;
  const checkRefetch = check.refetch;
  // "Recheck" waits on the local list; the version check just gets
  // restarted alongside it opportunistically, without blocking it: waiting
  // for it to finish before calling the refresh done would put back the
  // very seconds this design just removed.
  const refetch = useCallback(() => {
    void checkRefetch();
    return toolsRefetch();
  }, [checkRefetch, toolsRefetch]);

  return {
    data,
    isPending: tools.isPending,
    isFetched: tools.isFetched,
    isFetching: tools.isFetching,
    isSuccess: tools.isSuccess,
    isError: tools.isError,
    dataUpdatedAt: tools.dataUpdatedAt,
    refetch,
    checkingVersions: check.isFetching,
  };
}

export function useToolUpdatePreviews(
  tools: readonly ToolId[],
  enabled: boolean,
  revision = 0,
): UseQueryResult<ToolUpdatePreview[], Error> {
  const unique = [...new Set(tools)];
  return useQuery({
    queryKey: toolKeys.updatePreview(unique, revision),
    queryFn: () => native.tools.updatePreview(unique),
    enabled: enabled && unique.length > 0,
    staleTime: 0,
    gcTime: 0,
    retry: false,
    refetchOnMount: "always",
    refetchOnWindowFocus: false,
  });
}

export function useToolVersionCatalog(
  tool: ToolId | null,
): UseQueryResult<ToolVersionCatalog, Error> {
  return useQuery({
    queryKey: toolKeys.versions(tool ?? ""),
    queryFn: () =>
      tool === null
        ? Promise.reject(new Error("A tool is required"))
        : native.tools.versionCatalog(tool),
    enabled: tool !== null,
    staleTime: 60_000,
  });
}

export function useToolUninstallPreview(
  tool: ToolId | null,
): UseQueryResult<ToolUninstallPreview, Error> {
  return useQuery({
    queryKey: toolKeys.uninstallPreview(tool ?? ""),
    queryFn: () =>
      tool === null
        ? Promise.reject(new Error("A tool is required"))
        : native.tools.uninstallPreview(tool),
    enabled: tool !== null,
    staleTime: 0,
    refetchOnMount: "always",
  });
}
