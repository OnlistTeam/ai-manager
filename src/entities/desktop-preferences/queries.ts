import {
  queryOptions,
  useMutation,
  useQuery,
  useQueryClient,
  type UseMutationResult,
  type UseQueryResult,
} from "@tanstack/react-query";
import { sessionCacheOptions } from "@/lib/query/sessionCache";
import { native, type DesktopPreferences } from "@/native";

export const desktopPreferencesKeys = {
  all: ["desktop-preferences"] as const,
  current: () => ["desktop-preferences", "current"] as const,
};

export function desktopPreferencesQueryOptions() {
  return queryOptions({
    queryKey: desktopPreferencesKeys.current(),
    queryFn: () => native.desktopPreferences.get(),
    ...sessionCacheOptions,
  });
}

export function useDesktopPreferences(): UseQueryResult<
  DesktopPreferences,
  Error
> {
  return useQuery(desktopPreferencesQueryOptions());
}

export function useSaveDesktopPreferences(): UseMutationResult<
  DesktopPreferences,
  Error,
  DesktopPreferences
> {
  const queryClient = useQueryClient();
  return useMutation({
    mutationFn: (settings) => native.desktopPreferences.save(settings),
    onSuccess: (saved) => {
      queryClient.setQueryData(desktopPreferencesKeys.current(), saved);
    },
  });
}
