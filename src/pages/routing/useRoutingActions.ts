import { useState } from "react";
import {
  useAddRoutingProvider,
  useRemoveRoutingProvider,
  useSetRoutingFailover,
  useSetRoutingTakeover,
  useStopAllRouting,
  useSwitchRoutingProvider,
  type ToolId,
} from "@/entities/routing";

export interface RoutingChange {
  tool: ToolId;
  routed: boolean;
}

/**
 * Every routing change the page offers, with one busy flag, one error and
 * the last per-tool switch (for the note under that tool's row).
 */
export function useRoutingActions() {
  const takeover = useSetRoutingTakeover();
  const failover = useSetRoutingFailover();
  const add = useAddRoutingProvider();
  const remove = useRemoveRoutingProvider();
  const switchProvider = useSwitchRoutingProvider();
  const stop = useStopAllRouting();
  const [lastChange, setLastChange] = useState<RoutingChange | null>(null);
  const mutations = [takeover, failover, add, remove, switchProvider, stop];

  function reset(): void {
    for (const mutation of mutations) mutation.reset();
  }

  return {
    busy: mutations.some((mutation) => mutation.isPending),
    error: mutations.find((mutation) => mutation.error)?.error ?? null,
    lastChange,
    reset,
    setRouted(tool: ToolId, routed: boolean): void {
      reset();
      setLastChange(null);
      takeover.mutate(
        { tool, enabled: routed },
        { onSuccess: () => setLastChange({ tool, routed }) },
      );
    },
    setFailover(tool: ToolId, enabled: boolean): void {
      reset();
      failover.mutate({ tool, enabled });
    },
    addToQueue(tool: ToolId, providerId: string): void {
      reset();
      add.mutate({ tool, providerId });
    },
    removeFromQueue(tool: ToolId, providerId: string): void {
      reset();
      remove.mutate({ tool, providerId });
    },
    switchTo(tool: ToolId, providerId: string): void {
      reset();
      switchProvider.mutate({ tool, providerId });
    },
    stopAll(onDone: () => void): void {
      reset();
      setLastChange(null);
      stop.mutate(undefined, { onSuccess: onDone });
    },
  };
}

export type RoutingActions = ReturnType<typeof useRoutingActions>;
