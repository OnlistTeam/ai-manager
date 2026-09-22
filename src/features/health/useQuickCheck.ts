import { useEffect, useMemo, useRef } from "react";
import { useMutation, useQueryClient } from "@tanstack/react-query";
import {
  clearProviderConnectivityBatch,
  useHealthSnapshot,
  useProviderConnectivity,
  type HealthSnapshot,
  type ProviderHealth,
} from "@/entities/health";
import { useLocalExtensionInventory } from "@/entities/extension";
import { isTerminal } from "@/entities/operation";
import { useTaskAvailability } from "@/features/task-center";
import { useToolInventory } from "@/entities/tool";
import { native, type ToolId } from "@/native";
import { installedHealthTools } from "./installedHealthTools";
import { aggregateQuickCheck, type QuickCheckSummary } from "./quickCheck";

interface ConnectionTarget {
  tool: ToolId;
  providerIds: readonly string[];
}

function connectionTargetsOf(
  providers: readonly ProviderHealth[],
): ConnectionTarget[] {
  const byTool = new Map<ToolId, Set<string>>();
  for (const provider of providers) {
    const ids = byTool.get(provider.tool) ?? new Set<string>();
    for (const target of provider.checkTargets) ids.add(target.providerId);
    byTool.set(provider.tool, ids);
  }
  return [...byTool.entries()].flatMap(([tool, providerIds]) =>
    providerIds.size > 0 ? [{ tool, providerIds: [...providerIds] }] : [],
  );
}

export function useQuickCheck() {
  const queryClient = useQueryClient();
  // Opening the app must not reach the network on its own (ADR-0043). The
  // local list is read as before — those are files on this machine — but the
  // latest-version lookup waits for the user to ask, which "Check again"
  // does through `refetch`.
  const tools = useToolInventory({ checkVersions: false });
  const installed = installedHealthTools(tools.data ?? []);
  const snapshot = useHealthSnapshot(installed, tools.isSuccess);
  const localExtensions = useLocalExtensionInventory();
  const connectivity = useProviderConnectivity();
  const tasks = useTaskAvailability();
  const operations = tasks.operations.data ?? [];
  const connectionTargets = useMemo(
    () => connectionTargetsOf(snapshot.data?.providers ?? []),
    [snapshot.data?.providers],
  );
  const connectionCheck = useMutation<void, Error, readonly ConnectionTarget[]>(
    {
      mutationFn: async (targets) => {
        const results = await Promise.allSettled(
          targets.map((target) => native.providers.testAll(target.tool)),
        );
        const failed = results.find(
          (result): result is PromiseRejectedResult =>
            result.status === "rejected",
        );
        if (failed) {
          throw failed.reason instanceof Error
            ? failed.reason
            : new Error("connection check could not start");
        }
      },
      onMutate: (targets) => {
        for (const target of targets) {
          clearProviderConnectivityBatch(
            queryClient,
            target.tool,
            target.providerIds,
          );
        }
      },
    },
  );
  const checkedTools = new Set(connectionTargets.map((target) => target.tool));
  const connectionOperationRunning = operations.some(
    (operation) =>
      operation.kind === "testProviders" &&
      operation.tool !== null &&
      checkedTools.has(operation.tool) &&
      !isTerminal(operation.status),
  );
  const isCheckingConnections =
    connectionCheck.isPending || connectionOperationRunning;

  const calculatedData: QuickCheckSummary | undefined = useMemo(
    () =>
      tools.data &&
      snapshot.data &&
      (localExtensions.data !== undefined || localExtensions.isError)
        ? aggregateQuickCheck(
            tools.data,
            snapshot.data,
            connectivity.data ?? {},
            localExtensions.isError ? null : (localExtensions.data ?? null),
          )
        : undefined,
    [
      connectivity.data,
      localExtensions.data,
      localExtensions.isError,
      snapshot.data,
      tools.data,
    ],
  );
  const sourceError = tools.isError || snapshot.isError;
  const isFetching =
    tools.isFetching || snapshot.isFetching || localExtensions.isFetching;
  const calculatedAt =
    calculatedData && tools.dataUpdatedAt > 0 && snapshot.dataUpdatedAt > 0
      ? Math.max(tools.dataUpdatedAt, snapshot.dataUpdatedAt)
      : null;
  const lastSuccessful = useRef<{
    data: QuickCheckSummary;
    checkedAt: number;
  } | null>(null);

  useEffect(() => {
    if (calculatedData && calculatedAt && !sourceError && !isFetching) {
      lastSuccessful.current = {
        data: calculatedData,
        checkedAt: calculatedAt,
      };
    }
  }, [calculatedAt, calculatedData, isFetching, sourceError]);

  // Preserve the complete last-known-good pair while either source refreshes
  // or fails. Never combine a new tools list with an old health snapshot.
  const preserved =
    lastSuccessful.current && (isFetching || sourceError)
      ? lastSuccessful.current
      : null;
  const data = preserved?.data ?? calculatedData;
  const checkedAt = data ? (preserved?.checkedAt ?? calculatedAt) : null;

  async function refetchSources(): Promise<HealthSnapshot | undefined> {
    // A failed background tool refresh can retain trusted data while its
    // Query status is error. Recheck the paired snapshot whenever that
    // trusted inventory exists so recovery cannot mix fresh tools with an
    // older health read.
    const [, , , refreshedSnapshot] = await Promise.all([
      tools.refetch(),
      localExtensions.refetch(),
      tasks.operations.refetch(),
      tools.data !== undefined ? snapshot.refetch() : Promise.resolve(null),
    ]);
    return refreshedSnapshot?.data;
  }

  return {
    data,
    tools,
    localExtensions,
    checkedAt,
    isPending:
      tools.isPending ||
      (tools.isSuccess && snapshot.isPending) ||
      localExtensions.isPending,
    isFetching,
    // A refetch failure must not erase a result that was already safe to show.
    isError: !data && sourceError,
    refreshError: Boolean(data && sourceError),
    isCheckingConnections,
    connectionCheckError: connectionCheck.isError,
    taskAvailability: tasks,
    refetch: async () => {
      await refetchSources();
    },
    /**
     * One user action: refresh every read authority, then test the saved
     * service addresses that the fresh snapshot reports. Nothing here runs
     * without that click.
     */
    recheck: async () => {
      if (isCheckingConnections) return;
      const fresh = await refetchSources();
      const targets = connectionTargetsOf(fresh?.providers ?? []);
      if (targets.length > 0 && !tasks.actionsBlocked) {
        connectionCheck.mutate(targets);
      }
    },
  };
}
