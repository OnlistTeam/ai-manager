import { useCallback, useEffect, useRef, type RefObject } from "react";
import { useProviders } from "@/entities/provider";
import type { ToolId } from "@/entities/tool";

/** Derives stale-safe presentation and focus recovery from one scoped provider Query. */
export function useProviderInventoryRecovery(
  active: ToolId | null,
  pageRef: RefObject<HTMLDivElement>,
) {
  const providers = useProviders(active);
  const { refetch: refetchProviders } = providers;
  const retryButtonRef = useRef<HTMLButtonElement>(null);
  const focusAfterRetry = useRef(false);
  const dataAvailable = providers.data !== undefined;
  const providerInitiallyLoading =
    active !== null && providers.isPending && !providers.isFetched;
  const providerUnavailable =
    active !== null && providers.isFetched && !dataAvailable;
  const providerRefreshFailed =
    active !== null && providers.isError && dataAvailable;

  const retryProviders = useCallback(() => {
    focusAfterRetry.current = true;
    void refetchProviders();
  }, [refetchProviders]);

  useEffect(() => {
    if (!focusAfterRetry.current || providers.isFetching) return;
    focusAfterRetry.current = false;

    // A successful initial retry removes its button. Move focus to the named
    // page only when the user did not choose another control in the meantime.
    if (providers.isSuccess && document.activeElement === document.body) {
      pageRef.current?.focus({ preventScroll: true });
      return;
    }

    // A repeated initial failure keeps the same retry context recoverable.
    if (providers.isError && document.activeElement === document.body) {
      retryButtonRef.current?.focus({ preventScroll: true });
    }
  }, [pageRef, providers.isError, providers.isFetching, providers.isSuccess]);

  return {
    providers,
    retryButtonRef,
    providerInitiallyLoading,
    providerUnavailable,
    providerRefreshFailed,
    providerActionsBlocked: providers.isFetching || providers.isError,
    retryProviders,
  };
}
