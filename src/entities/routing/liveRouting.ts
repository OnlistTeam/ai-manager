import { useEffect } from "react";
import {
  useQuery,
  useQueryClient,
  type UseQueryResult,
} from "@tanstack/react-query";
import { native, onRoutingTrace, type RoutingTraceSnapshot } from "@/native";
import { routingKeys } from "./queries";
import { mergeRoutingTraceSnapshot, mergeRoutingTraceUpdate } from "./trace";

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
    queryFn: async () => {
      const read = await native.routing.trace();
      // Read the cache only now: changes pushed while the read was in
      // flight are already in it and must survive the seed.
      return mergeRoutingTraceSnapshot(
        queryClient.getQueryData<RoutingTraceSnapshot>(routingKeys.trace()),
        read,
      );
    },
    staleTime: Number.POSITIVE_INFINITY,
    refetchOnWindowFocus: false,
  });
}
