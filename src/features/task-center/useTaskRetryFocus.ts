import { useCallback, useEffect, useRef, type RefObject } from "react";

interface RetryableOperations {
  refetch: () => Promise<unknown>;
  isFetching: boolean;
  isError: boolean;
}

/**
 * Retry for the task list, returning focus to whichever control makes sense
 * afterwards: the retry button if it is still needed, the panel title if the
 * retry succeeded and the button went away.
 */
export function useTaskRetryFocus(
  operations: RetryableOperations,
  titleRef: RefObject<HTMLHeadingElement>,
) {
  const retryButtonRef = useRef<HTMLButtonElement>(null);
  const focusAfterRetry = useRef(false);
  const { refetch: refetchOperations } = operations;

  const retryTasks = useCallback(() => {
    focusAfterRetry.current = true;
    void refetchOperations();
  }, [refetchOperations]);

  useEffect(() => {
    if (!focusAfterRetry.current || operations.isFetching) return;
    focusAfterRetry.current = false;

    // Respect focus deliberately moved elsewhere. A successful retry removes
    // its button, leaving body focused; hand that context back to the panel.
    if (document.activeElement !== document.body) return;
    if (operations.isError) {
      retryButtonRef.current?.focus({ preventScroll: true });
    } else {
      titleRef.current?.focus({ preventScroll: true });
    }
  }, [operations.isError, operations.isFetching, titleRef]);

  return { retryButtonRef, retryTasks };
}
