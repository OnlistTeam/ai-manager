import { useCallback, useEffect, useRef } from "react";
import { useOperations } from "@/entities/operation";
import { useQuickCheck } from "@/features/health";

/** Coordinates the two read authorities that gate trusted Home actions. */
export function useHomeReadinessRecovery() {
  const check = useQuickCheck();
  const operations = useOperations();
  const { refetch: refetchCheck } = check;
  const { refetch: refetchOperations } = operations;
  const pageRef = useRef<HTMLDivElement>(null);
  const retryButtonRef = useRef<HTMLButtonElement>(null);
  const focusAfterRetry = useRef(false);

  // TanStack returns an empty failed Query to pending while it refetches.
  // Preserve the explicit recovery surface without copying the Query data.
  const retryingInitialCheck = focusAfterRetry.current && !check.data;
  const checkUnavailable = check.isError || retryingInitialCheck;
  const sourcesUnavailable =
    checkUnavailable || check.refreshError || operations.isError;
  const sourcesPending = check.isPending || operations.isPending;
  const retrying = check.isFetching || operations.isFetching;
  const refreshFailed = !checkUnavailable && sourcesUnavailable;
  const actionsBlocked = sourcesUnavailable || sourcesPending || retrying;

  const retryHome = useCallback(() => {
    focusAfterRetry.current = true;
    void Promise.allSettled([refetchCheck(), refetchOperations()]);
  }, [refetchCheck, refetchOperations]);

  useEffect(() => {
    if (!focusAfterRetry.current || retrying) return;
    focusAfterRetry.current = false;

    // A user who moved to another safe action owns focus. Otherwise return to
    // the surviving Retry or to the named page after authoritative recovery.
    if (document.activeElement !== document.body) return;
    if (sourcesUnavailable) {
      retryButtonRef.current?.focus({ preventScroll: true });
      return;
    }
    pageRef.current?.focus({ preventScroll: true });
  }, [retrying, sourcesUnavailable]);

  return {
    check,
    operations,
    pageRef,
    retryButtonRef,
    checkUnavailable,
    refreshFailed,
    sourcesUnavailable,
    sourcesPending,
    retrying,
    actionsBlocked,
    retryHome,
  };
}
