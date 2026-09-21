import { useCallback, useSyncExternalStore } from "react";
import { hashKey, useQueryClient } from "@tanstack/react-query";
import type { Operation } from "@/native";
import { operationKeys } from "./keys";

const EMPTY_OPERATIONS: Operation[] = [];
const OPERATIONS_HASH = hashKey(operationKeys.list());

/**
 * Observes the operation cache without fetching or seeding it. Task Center owns
 * the native baseline query; feature pages only need to react to the shared
 * operation event stream.
 */
export function useCachedOperations(): readonly Operation[] {
  const queryClient = useQueryClient();
  const subscribe = useCallback(
    (notify: () => void) =>
      queryClient.getQueryCache().subscribe((event) => {
        if (event.query.queryHash === OPERATIONS_HASH) {
          notify();
        }
      }),
    [queryClient],
  );
  const snapshot = useCallback(
    () =>
      queryClient.getQueryData<Operation[]>(operationKeys.list()) ??
      EMPTY_OPERATIONS,
    [queryClient],
  );
  return useSyncExternalStore(subscribe, snapshot, snapshot);
}
