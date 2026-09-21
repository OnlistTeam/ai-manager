import { useCallback, useEffect, useRef, type RefObject } from "react";
import { useSkillCatalog } from "@/entities/skill-catalog";
import type { ToolId } from "@/entities/tool";

/** Derives stale-safe catalog presentation and focus from its authoritative query. */
export function useSkillCatalogRecovery(
  tool: ToolId,
  open: boolean,
  successFocusRef: RefObject<HTMLElement>,
) {
  const catalog = useSkillCatalog(tool, open);
  const { refetch: refetchCatalog } = catalog;
  const retryButtonRef = useRef<HTMLButtonElement>(null);
  const focusAfterRetry = useRef(false);
  const catalogAvailable = catalog.data !== undefined;

  const retryCatalog = useCallback(() => {
    focusAfterRetry.current = true;
    void refetchCatalog();
  }, [refetchCatalog]);

  useEffect(() => {
    focusAfterRetry.current = false;
  }, [open, tool]);

  useEffect(() => {
    if (!focusAfterRetry.current || catalog.isFetching) return;
    focusAfterRetry.current = false;

    const active = document.activeElement;
    if (active !== document.body && active !== retryButtonRef.current) return;
    if (catalog.isError) {
      retryButtonRef.current?.focus({ preventScroll: true });
    } else if (catalog.isSuccess) {
      successFocusRef.current?.focus({ preventScroll: true });
    }
  }, [catalog.isError, catalog.isFetching, catalog.isSuccess, successFocusRef]);

  return {
    catalog,
    retryButtonRef,
    catalogAvailable,
    catalogInitiallyLoading: catalog.isPending && !catalog.isFetched,
    catalogUnavailable: catalog.isFetched && !catalogAvailable,
    catalogRefreshFailed: catalog.isError && catalogAvailable,
    catalogRefreshingRetained:
      catalog.isFetching && catalogAvailable && !catalog.isError,
    catalogActionsBlocked:
      !catalogAvailable || catalog.isFetching || catalog.isError,
    retryCatalog,
  };
}
