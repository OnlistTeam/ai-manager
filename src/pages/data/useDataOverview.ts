import { useQueries } from "@tanstack/react-query";
import { providerRuntimeContextQueryOptions } from "@/entities/provider";
import type { Tool, ToolId } from "@/entities/tool";

export interface DataOverview {
  /** Total volume of listed local data, summed across every manageable tool (bytes). */
  totalBytes: number;
  /** Conversation data volume, summed across every manageable tool (bytes). */
  conversationBytes: number;
  /** At least one tool's runtime context hasn't come back yet: neither data nor an error. */
  pending: boolean;
  /** Tools that failed to read and have no usable data; their volume isn't included in the total. */
  failedTools: readonly ToolId[];
  /** The total is only a lower bound: some tool failed to read, or some tool's own measurement was truncated. */
  lowerBound: boolean;
}

/**
 * Each tool's runtime context was already warmed at startup by
 * `warmSessionEnvironment` (`sessionCacheOptions`'s staleTime/gcTime are
 * both Infinity), so `useQueries` here is only reading answers already in
 * the cache and summing them — it doesn't issue new requests. The two
 * fields each correspond to the two remaining cells of the single-tool
 * panel (`ProviderRuntimeStorageSummary`), just switched from "this tool"
 * to "summed across every tool".
 *
 * A tool that hasn't come back yet must not be silently counted as 0:
 * "0 B" is an assertion, while "measuring" and "≥" are the honest answers.
 */
export function useDataOverview(manageable: readonly Tool[]): DataOverview {
  const results = useQueries({
    queries: manageable.map((tool) =>
      providerRuntimeContextQueryOptions(tool.id),
    ),
  });

  let totalBytes = 0;
  let conversationBytes = 0;
  let pending = false;
  let limited = false;
  const failedTools: ToolId[] = [];

  manageable.forEach((tool, index) => {
    const result = results[index];
    if (result?.data) {
      totalBytes += result.data.storage.totalBytes;
      conversationBytes += result.data.storage.sessionBytes;
      limited ||= result.data.storage.measurementLimited;
      return;
    }
    if (result?.isError) {
      failedTools.push(tool.id);
      return;
    }
    pending = true;
  });

  return {
    totalBytes,
    conversationBytes,
    pending,
    failedTools,
    lowerBound: limited || failedTools.length > 0,
  };
}
