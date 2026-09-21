import { useCallback, useEffect, useRef } from "react";
import { useBackups } from "@/entities/backup";

/** Derives recovery presentation and focus from the one authoritative backup query. */
export function useBackupInventoryRecovery() {
  const backups = useBackups();
  const { refetch: refetchBackups } = backups;
  const sectionRef = useRef<HTMLElement>(null);
  const retryButtonRef = useRef<HTMLButtonElement>(null);
  const focusAfterRetry = useRef(false);
  const inventoryAvailable = backups.data !== undefined;

  const retryInventory = useCallback(() => {
    focusAfterRetry.current = true;
    void refetchBackups();
  }, [refetchBackups]);

  useEffect(() => {
    if (!focusAfterRetry.current || backups.isFetching) return;
    focusAfterRetry.current = false;

    const active = document.activeElement;
    if (active !== document.body && active !== retryButtonRef.current) return;
    if (backups.isSuccess) {
      sectionRef.current?.focus({ preventScroll: true });
    } else if (backups.isError) {
      retryButtonRef.current?.focus({ preventScroll: true });
    }
  }, [backups.isError, backups.isFetching, backups.isSuccess]);

  return {
    backups,
    sectionRef,
    retryButtonRef,
    inventoryAvailable,
    inventoryInitiallyLoading: backups.isPending && !backups.isFetched,
    inventoryUnavailable: backups.isFetched && !inventoryAvailable,
    inventoryRefreshFailed: backups.isError && inventoryAvailable,
    inventoryActionsBlocked: backups.isFetching || backups.isError,
    retryInventory,
  };
}
