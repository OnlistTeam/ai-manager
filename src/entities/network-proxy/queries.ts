import {
  queryOptions,
  useMutation,
  useQuery,
  useQueryClient,
  type UseMutationResult,
  type UseQueryResult,
} from "@tanstack/react-query";
import { sessionCacheOptions } from "@/lib/query/sessionCache";
import { native, type NetworkProxySettings } from "@/native";

export const networkProxyKeys = {
  all: ["network-proxy"] as const,
  current: () => ["network-proxy", "current"] as const,
};

export function networkProxyQueryOptions() {
  return queryOptions({
    queryKey: networkProxyKeys.current(),
    queryFn: () => native.networkProxy.get(),
    ...sessionCacheOptions,
  });
}

export function useNetworkProxy(): UseQueryResult<NetworkProxySettings, Error> {
  return useQuery(networkProxyQueryOptions());
}

export function useSaveNetworkProxy(): UseMutationResult<
  NetworkProxySettings,
  Error,
  string | null
> {
  const queryClient = useQueryClient();
  return useMutation({
    mutationFn: (url) => native.networkProxy.save(url),
    onSuccess: (settings) => {
      queryClient.setQueryData(networkProxyKeys.current(), settings);
    },
  });
}
