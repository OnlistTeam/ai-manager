import { useCallback, useEffect, useRef } from "react";
import { useToolInventory } from "@/entities/tool";

/** Derives presentation and focus recovery from the single authoritative Tools Query. */
export function useToolInventoryRecovery() {
  const tools = useToolInventory();
  const { refetch: refetchTools } = tools;
  const pageRef = useRef<HTMLDivElement>(null);
  const focusAfterRetry = useRef(false);
  const inventoryInitiallyLoading = tools.isPending && !tools.isFetched;
  const inventoryUnavailable = tools.isFetched && tools.data === undefined;
  const inventoryRefreshFailed = tools.isError && tools.data !== undefined;

  const retryInventory = useCallback(() => {
    focusAfterRetry.current = true;
    void refetchTools();
  }, [refetchTools]);

  useEffect(() => {
    if (!focusAfterRetry.current || tools.isFetching) return;
    focusAfterRetry.current = false;
    if (tools.isSuccess) pageRef.current?.focus({ preventScroll: true });
  }, [tools.isFetching, tools.isSuccess]);

  return {
    tools,
    pageRef,
    inventoryInitiallyLoading,
    inventoryUnavailable,
    inventoryRefreshFailed,
    inventoryActionsBlocked:
      tools.data === undefined || tools.isFetching || tools.isError,
    // While the background fetch fills in the latest version number, cards
    // stay clickable as usual: a version check is not a reason to block.
    checkingVersions: tools.checkingVersions,
    retryInventory,
  };
}
