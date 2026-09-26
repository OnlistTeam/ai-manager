import {
  queryOptions,
  useMutation,
  useQuery,
  useQueryClient,
  type UseMutationResult,
  type UseQueryResult,
} from "@tanstack/react-query";
import { sessionCacheOptions } from "@/lib/query/sessionCache";
import { native, type PrivacyProtection } from "@/native";

export const privacyProtectionKeys = {
  all: ["privacy-protection"] as const,
  current: () => ["privacy-protection", "current"] as const,
};

export function privacyProtectionQueryOptions() {
  return queryOptions({
    queryKey: privacyProtectionKeys.current(),
    queryFn: () => native.privacyProtection.get(),
    ...sessionCacheOptions,
  });
}

export function usePrivacyProtection(): UseQueryResult<
  PrivacyProtection,
  Error
> {
  return useQuery(privacyProtectionQueryOptions());
}

/** Saves the switch; the cache takes the value the backend read back. */
export function useSetPrivacyProtection(): UseMutationResult<
  PrivacyProtection,
  Error,
  boolean
> {
  const queryClient = useQueryClient();
  return useMutation({
    mutationFn: (enabled) => native.privacyProtection.set(enabled),
    onSuccess: (saved) => {
      queryClient.setQueryData(privacyProtectionKeys.current(), saved);
    },
  });
}
