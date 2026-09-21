import {
  useMutation,
  useQuery,
  useQueryClient,
  type UseMutationResult,
  type UseQueryResult,
} from "@tanstack/react-query";
import { providerKeys } from "@/entities/provider";
import { sessionCacheOptions } from "@/lib/query/sessionCache";
import { native, type RoutingOverview, type ToolId } from "@/native";

export const routingKeys = {
  all: ["routing"] as const,
  overview: () => ["routing", "overview"] as const,
};

export interface RoutingToggleInput {
  tool: ToolId;
  enabled: boolean;
}

export interface RoutingProviderInput {
  tool: ToolId;
  providerId: string;
}

export function useRoutingOverview(): UseQueryResult<RoutingOverview, Error> {
  return useQuery({
    queryKey: routingKeys.overview(),
    queryFn: () => native.routing.overview(),
    ...sessionCacheOptions,
    refetchInterval: 10_000,
  });
}

function useOverviewMutation<TInput>(
  mutationFn: (input: TInput) => Promise<RoutingOverview>,
): UseMutationResult<RoutingOverview, Error, TInput> {
  const queryClient = useQueryClient();
  return useMutation({
    mutationFn,
    onSuccess: (overview) => {
      queryClient.setQueryData(routingKeys.overview(), overview);
      void queryClient.invalidateQueries({ queryKey: providerKeys.all });
    },
  });
}

export function useSetRoutingTakeover() {
  return useOverviewMutation(({ tool, enabled }: RoutingToggleInput) =>
    native.routing.setTakeover(tool, enabled),
  );
}

export function useSetRoutingFailover() {
  return useOverviewMutation(({ tool, enabled }: RoutingToggleInput) =>
    native.routing.setFailover(tool, enabled),
  );
}

export function useAddRoutingProvider() {
  return useOverviewMutation(({ tool, providerId }: RoutingProviderInput) =>
    native.routing.addToQueue(tool, providerId),
  );
}

export function useRemoveRoutingProvider() {
  return useOverviewMutation(({ tool, providerId }: RoutingProviderInput) =>
    native.routing.removeFromQueue(tool, providerId),
  );
}

export function useSwitchRoutingProvider() {
  return useOverviewMutation(({ tool, providerId }: RoutingProviderInput) =>
    native.routing.switchProvider(tool, providerId),
  );
}

export function useStopAllRouting() {
  return useOverviewMutation(() => native.routing.stopAll());
}
