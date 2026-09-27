import { useCallback, useState } from "react";
import { useQueryClient } from "@tanstack/react-query";
import {
  isTerminal,
  operationKeys,
  useCachedOperations,
} from "@/entities/operation";

export type DiscoverAddState = "idle" | "adding" | "added";

export interface DiscoverAddFailure {
  name: string;
  error: unknown;
}

export interface DiscoverAdds {
  /**
   * Runs one add. Resolves to `null` once the task was accepted, else to the
   * refusal, which is also shown in the section unless `quiet`.
   */
  start: (
    id: string,
    name: string,
    run: () => Promise<string>,
    quiet?: boolean,
  ) => Promise<unknown>;
  stateOf: (id: string, added: boolean) => DiscoverAddState;
  failure: DiscoverAddFailure | null;
  dismissFailure: () => void;
}

/**
 * Which cards are being added. An add is a background task: the card says
 * "Adding…" until its task finishes, then "Added" (the list above gains the
 * row when the task's end refreshes the inventory). A task that fails puts
 * the Add button back; its failure is shown with the task above the list.
 * A request refused before any task started is shown once, in the section.
 */
export function useDiscoverAdds(): DiscoverAdds {
  const queryClient = useQueryClient();
  const operations = useCachedOperations();
  const [pending, setPending] = useState<Record<string, string | null>>({});
  const [failure, setFailure] = useState<DiscoverAddFailure | null>(null);

  const start = useCallback(
    async (
      id: string,
      name: string,
      run: () => Promise<string>,
      quiet = false,
    ) => {
      setFailure(null);
      setPending((current) => ({ ...current, [id]: null }));
      try {
        const operation = await run();
        setPending((current) => ({ ...current, [id]: operation }));
        void queryClient.invalidateQueries({ queryKey: operationKeys.all });
        return null;
      } catch (error) {
        setPending((current) => {
          const next = { ...current };
          delete next[id];
          return next;
        });
        if (!quiet) setFailure({ name, error });
        return error;
      }
    },
    [queryClient],
  );

  const stateOf = useCallback(
    (id: string, added: boolean): DiscoverAddState => {
      if (added) return "added";
      if (!(id in pending)) return "idle";
      const operationId = pending[id];
      const operation =
        operationId === null
          ? undefined
          : operations.find((candidate) => candidate.id === operationId);
      if (!operation) return "adding";
      if (operation.status === "success") return "added";
      return isTerminal(operation.status) ? "idle" : "adding";
    },
    [operations, pending],
  );

  const dismissFailure = useCallback(() => setFailure(null), []);

  return { start, stateOf, failure, dismissFailure };
}
