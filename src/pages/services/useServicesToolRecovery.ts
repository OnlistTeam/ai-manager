import { useCallback, useEffect, useRef } from "react";
import { useToolInventory } from "@/entities/tool";

/** Keeps the last authoritative tool inventory readable while detection recovers. */
export function useServicesToolRecovery() {
  const tools = useToolInventory();
  const { refetch: refetchTools } = tools;
  const pageRef = useRef<HTMLDivElement>(null);
  const retryButtonRef = useRef<HTMLButtonElement>(null);
  const focusAfterRetry = useRef(false);
  const dataAvailable = tools.data !== undefined;
  const initiallyLoading = tools.isPending && !tools.isFetched;
  const unavailable = tools.isFetched && !dataAvailable;
  const refreshFailed = tools.isError && dataAvailable;

  const retryTools = useCallback(() => {
    focusAfterRetry.current = true;
    void refetchTools();
  }, [refetchTools]);

  useEffect(() => {
    if (!focusAfterRetry.current || tools.isFetching) return;
    focusAfterRetry.current = false;
    if (document.activeElement !== document.body) return;

    if (tools.isSuccess) {
      pageRef.current?.focus({ preventScroll: true });
      return;
    }
    if (tools.isError) {
      retryButtonRef.current?.focus({ preventScroll: true });
    }
  }, [tools.isError, tools.isFetching, tools.isSuccess]);

  return {
    tools,
    pageRef,
    retryButtonRef,
    initiallyLoading,
    unavailable,
    refreshFailed,
    actionsBlocked: tools.isFetching || tools.isError,
    retryTools,
  };
}
