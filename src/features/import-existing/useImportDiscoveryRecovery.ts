import { useCallback, useEffect, useRef } from "react";
import { useImportPreview } from "@/entities/import";

/** Derives Settings recovery state from the single authoritative import preview. */
export function useImportDiscoveryRecovery() {
  const preview = useImportPreview();
  const { refetch: refetchPreview } = preview;
  const sectionRef = useRef<HTMLElement>(null);
  const retryButtonRef = useRef<HTMLButtonElement>(null);
  const focusAfterRetry = useRef(false);
  const discoveryAvailable = preview.data !== undefined;

  const focusDiscoveryResult = useCallback(() => {
    if (retryButtonRef.current?.isConnected) {
      retryButtonRef.current.focus({ preventScroll: true });
    } else {
      sectionRef.current?.focus({ preventScroll: true });
    }
  }, []);

  const retryDiscovery = useCallback(() => {
    focusAfterRetry.current = true;
    void refetchPreview();
  }, [refetchPreview]);

  useEffect(() => {
    if (!focusAfterRetry.current || preview.isFetching) return;
    focusAfterRetry.current = false;

    const active = document.activeElement;
    if (active !== document.body && active !== retryButtonRef.current) return;
    if (preview.isSuccess || preview.isError) focusDiscoveryResult();
  }, [
    focusDiscoveryResult,
    preview.isError,
    preview.isFetching,
    preview.isSuccess,
  ]);

  return {
    preview,
    sectionRef,
    retryButtonRef,
    discoveryAvailable,
    discoveryInitiallyLoading: preview.isPending && !preview.isFetched,
    discoveryUnavailable: preview.isFetched && !discoveryAvailable,
    discoveryRefreshFailed: preview.isError && discoveryAvailable,
    discoveryActionsBlocked: preview.isFetching || preview.isError,
    retryDiscovery,
    focusDiscoveryResult,
  };
}
