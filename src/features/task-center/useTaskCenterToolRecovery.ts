import { useCallback, useEffect, useRef, type RefObject } from "react";
import { useToolInventory } from "@/entities/tool";

/** Keeps task history readable without letting stale tool capabilities authorize Open. */
export function useTaskCenterToolRecovery(
  focusTargetRef: RefObject<HTMLHeadingElement>,
) {
  const tools = useToolInventory();
  const { refetch: refetchTools } = tools;
  const retryButtonRef = useRef<HTMLButtonElement>(null);
  const focusAfterRetry = useRef(false);
  const dataAvailable = tools.data !== undefined;
  const unavailable = tools.isFetched && !dataAvailable;
  const refreshFailed = tools.isError && dataAvailable;

  const retryTools = useCallback(() => {
    focusAfterRetry.current = true;
    void refetchTools();
  }, [refetchTools]);

  useEffect(() => {
    if (!focusAfterRetry.current || tools.isFetching) return;
    focusAfterRetry.current = false;

    const active = document.activeElement;
    if (active !== document.body && active !== retryButtonRef.current) return;
    if (tools.isError) {
      retryButtonRef.current?.focus({ preventScroll: true });
      return;
    }
    if (tools.isSuccess) {
      focusTargetRef.current?.focus({ preventScroll: true });
    }
  }, [focusTargetRef, tools.isError, tools.isFetching, tools.isSuccess]);

  return {
    tools,
    retryButtonRef,
    unavailable,
    refreshFailed,
    actionsBlocked: !dataAvailable || tools.isFetching || tools.isError,
    retryTools,
  };
}
