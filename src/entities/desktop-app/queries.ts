import {
  queryOptions,
  useMutation,
  useQuery,
  type UseMutationResult,
  type UseQueryResult,
} from "@tanstack/react-query";
import {
  native,
  type DesktopApp,
  type DesktopAppId,
  type DesktopAppLaunchOutcome,
  type DesktopAppOfficialDownloadOutcome,
  type DesktopAppUninstallOutcome,
} from "@/native";
import { sessionCacheOptions } from "@/lib/query/sessionCache";

export const desktopAppKeys = {
  all: ["desktop-apps"] as const,
  list: () => ["desktop-apps", "list"] as const,
};

export function desktopAppsQueryOptions() {
  return queryOptions({
    queryKey: desktopAppKeys.list(),
    queryFn: () => native.desktopApps.list(),
    ...sessionCacheOptions,
  });
}

export function useDesktopApps(): UseQueryResult<DesktopApp[], Error> {
  return useQuery(desktopAppsQueryOptions());
}

export function useLaunchDesktopApp(): UseMutationResult<
  DesktopAppLaunchOutcome,
  Error,
  DesktopAppId
> {
  return useMutation({
    mutationFn: (app) => native.desktopApps.launch(app),
  });
}

export function useOpenDesktopAppOfficialDownload(): UseMutationResult<
  DesktopAppOfficialDownloadOutcome,
  Error,
  DesktopAppId
> {
  return useMutation({
    mutationFn: (app) => native.desktopApps.openOfficialDownload(app),
  });
}

export function useOpenDesktopAppUninstall(): UseMutationResult<
  DesktopAppUninstallOutcome,
  Error,
  DesktopAppId
> {
  return useMutation({
    mutationFn: (app) => native.desktopApps.openUninstall(app),
  });
}
