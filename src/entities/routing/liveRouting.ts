import { useEffect } from "react";
import {
  useMutation,
  useQuery,
  useQueryClient,
  type UseMutationResult,
  type UseQueryResult,
} from "@tanstack/react-query";
import { providerKeys } from "@/entities/provider";
import {
  native,
  onRoutingTrace,
  type RoutingLiveModeOutcome,
  type RoutingOverview,
  type RoutingTraceSnapshot,
} from "@/native";
import { routingKeys } from "./queries";
import { mergeRoutingTraceSnapshot, mergeRoutingTraceUpdate } from "./trace";

/** Live routing is on while the route runs and at least one tool is taken over. */
export function isLiveRoutingOn(overview: RoutingOverview): boolean {
  return (
    overview.running &&
    overview.targets.some((target) => target.takeoverEnabled)
  );
}

/**
 * The recent requests the local route handled, newest first (ADR-0050).
 * Seeds from the native ring once the push subscription is attached, then
 * folds each pushed change into the same cache entry.
 */
export function useRoutingTrace(): UseQueryResult<RoutingTraceSnapshot, Error> {
  const queryClient = useQueryClient();

  useEffect(() => {
    let disposed = false;
    const subscription = onRoutingTrace((update) => {
      queryClient.setQueryData<RoutingTraceSnapshot>(
        routingKeys.trace(),
        (current) => mergeRoutingTraceUpdate(current, update),
      );
    });
    // Anything pushed before the listener existed is picked up by one read.
    void subscription.then(
      () => {
        if (!disposed) {
          void queryClient.invalidateQueries({ queryKey: routingKeys.trace() });
        }
      },
      (error: unknown) => {
        console.error("Could not subscribe to the routing trace", error);
      },
    );

    return () => {
      disposed = true;
      void subscription.then(
        (unlisten) => unlisten(),
        () => undefined,
      );
    };
  }, [queryClient]);

  return useQuery({
    queryKey: routingKeys.trace(),
    queryFn: async () =>
      mergeRoutingTraceSnapshot(
        queryClient.getQueryData<RoutingTraceSnapshot>(routingKeys.trace()),
        await native.routing.trace(),
      ),
    staleTime: Number.POSITIVE_INFINITY,
    refetchOnWindowFocus: false,
  });
}

export function useSetLiveRoutingMode(): UseMutationResult<
  RoutingLiveModeOutcome,
  Error,
  boolean
> {
  const queryClient = useQueryClient();
  return useMutation({
    mutationKey: routingKeys.liveMode(),
    mutationFn: (enabled: boolean) => native.routing.setLiveMode(enabled),
    onSuccess: ({ overview }) => {
      queryClient.setQueryData(routingKeys.overview(), overview);
      void queryClient.invalidateQueries({ queryKey: providerKeys.all });
    },
  });
}
