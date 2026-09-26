import {
  queryOptions,
  useMutation,
  useQuery,
  useQueryClient,
  type UseMutationResult,
  type UseQueryResult,
} from "@tanstack/react-query";
import { sessionCacheOptions } from "@/lib/query/sessionCache";
import {
  native,
  type PrivacyProtection,
  type PrivacyProtectionPatch,
} from "@/native";

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

/** Saves a partial change; the cache takes every value the backend read back. */
export function useSetPrivacyProtection(): UseMutationResult<
  PrivacyProtection,
  Error,
  PrivacyProtectionPatch
> {
  const queryClient = useQueryClient();
  return useMutation({
    mutationFn: (patch) => native.privacyProtection.set(patch),
    onSuccess: (saved) => {
      queryClient.setQueryData(privacyProtectionKeys.current(), saved);
    },
  });
}
