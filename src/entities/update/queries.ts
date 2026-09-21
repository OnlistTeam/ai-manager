import {
  useMutation,
  useQuery,
  useQueryClient,
  type UseMutationResult,
  type UseQueryResult,
} from "@tanstack/react-query";
import {
  checkForUpdate,
  installAppUpdateAndRestart,
  openAppDownloadPage,
  startAppUpdate,
  type UpdateStatus,
} from "@/native/updater";

export const updateKeys = {
  all: ["update"] as const,
  status: () => ["update", "status"] as const,
};

const RECHECK_AFTER_MS = 6 * 60 * 60 * 1000;

/**
 * AppRoot starts one backend-owned check. While it is checking/downloading,
 * short status polling keeps Settings responsive without starting duplicate
 * downloads. The signed package remains in the backend until restart.
 *
 * Once the backend answers "up to date" that answer is terminal until it
 * ages past `RECHECK_AFTER_MS`, which the backend enforces independently.
 * Refetching on the same schedule is what makes a long-lived window notice a
 * release published after launch: returning to the app triggers it through
 * `staleTime`, and the interval covers a window nobody ever leaves.
 */
export function useUpdateStatus(): UseQueryResult<UpdateStatus, Error> {
  return useQuery({
    queryKey: updateKeys.status(),
    queryFn: checkForUpdate,
    staleTime: RECHECK_AFTER_MS,
    refetchInterval: (query) => {
      const phase = query.state.data?.phase;
      if (phase === "checking" || phase === "downloading") return 500;
      return phase === "upToDate" ? RECHECK_AFTER_MS : false;
    },
    refetchIntervalInBackground: true,
    retry: false,
  });
}

export function useCheckForUpdate(): UseMutationResult<
  UpdateStatus,
  Error,
  void
> {
  const client = useQueryClient();
  return useMutation({
    mutationFn: () => startAppUpdate(true),
    onSuccess: (status) => client.setQueryData(updateKeys.status(), status),
  });
}

export function useInstallAppUpdate(): UseMutationResult<boolean, Error, void> {
  return useMutation({ mutationFn: installAppUpdateAndRestart });
}

export function useOpenAppDownloadPage(): UseMutationResult<
  boolean,
  Error,
  void
> {
  return useMutation({ mutationFn: openAppDownloadPage });
}
