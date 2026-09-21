import {
  useMutation,
  useQueryClient,
  type UseMutationResult,
} from "@tanstack/react-query";
import { useTranslation } from "react-i18next";
import { toast } from "sonner";
import {
  native,
  type Operation,
  type ToolId,
  type ToolLaunchDirectoryMode,
  type ToolLaunchOutcome,
  type UninstallOptions,
} from "@/native";
import { mergeOperation, operationKeys } from "@/entities/operation";
import { toErrorCopy } from "@/shared/lib/nativeError";

export interface UpdateToolVariables {
  tool: ToolId;
  previewFingerprint: string;
}

export interface UninstallVariables {
  tool: ToolId;
  options: UninstallOptions;
}

export interface InstallVersionVariables {
  tool: ToolId;
  version: string;
}

export interface LifecycleMutationOptions {
  /** A caller with durable inline feedback can suppress the transient toast. */
  notifyOnError?: boolean;
}

export interface LaunchMutationOptions {
  /** A caller with durable dialog feedback can suppress the transient toast. */
  notifyOnError?: boolean;
}

export interface LaunchToolVariables {
  tool: ToolId;
  directoryMode: ToolLaunchDirectoryMode;
}

/**
 * Shared finish-up for the three write operations: success only means "the
 * task has been queued" (the backend's begin acquires the lock
 * synchronously) — progress and result are both pushed via
 * `operation://changed`. When it fails before the task even starts, per §42
 * this surfaces only human-readable copy + remediation; the caller can
 * choose in-page presentation or a transient toast, and technical fields
 * never reach the ordinary UI.
 */
function useLifecycleMutation<TVariables>(
  run: (variables: TVariables) => Promise<string>,
  options: LifecycleMutationOptions = {},
): UseMutationResult<string, Error, TVariables> {
  const queryClient = useQueryClient();
  const notify = useLifecycleErrorNotice();

  return useMutation<string, Error, TVariables>({
    mutationFn: run,
    // Success and begin-time failures both refresh the task authority. A stale
    // empty baseline must not leave an OPERATION_CONFLICT retry enabled.
    onSettled: () =>
      queryClient.invalidateQueries({ queryKey: operationKeys.all }),
    onError: (error) => {
      if (options.notifyOnError === false) return;
      notify(error);
    },
  });
}

/**
 * Spec §42's transient failure toast: human-readable copy + remediation,
 * with technical fields left for "View Details".
 *
 * Even a caller that opted for in-page presentation (`notifyOnError: false`)
 * still needs this: the user may close that dialog before the handoff
 * completes, so the in-page error becomes invisible to anyone, and this
 * toast is the only thing that still gets delivered.
 */
export function useLifecycleErrorNotice(): (error: Error) => void {
  const { t } = useTranslation();
  return (error) => {
    const copy = toErrorCopy(error);
    toast.error(t(copy.messageKey), {
      description: copy.remediationKey ? t(copy.remediationKey) : undefined,
    });
  };
}

export function useInstallTool(
  options?: LifecycleMutationOptions,
): UseMutationResult<string, Error, ToolId> {
  return useLifecycleMutation<ToolId>(
    (tool) => native.tools.install(tool),
    options,
  );
}

export function useUpdateTool(
  options?: LifecycleMutationOptions,
): UseMutationResult<string, Error, UpdateToolVariables> {
  return useLifecycleMutation<UpdateToolVariables>(
    ({ tool, previewFingerprint }) =>
      native.tools.update(tool, previewFingerprint),
    options,
  );
}

export function useInstallToolVersion(
  options?: LifecycleMutationOptions,
): UseMutationResult<string, Error, InstallVersionVariables> {
  return useLifecycleMutation<InstallVersionVariables>(
    ({ tool, version }) => native.tools.installVersion(tool, version),
    options,
  );
}

export function useRepairTool(
  options?: LifecycleMutationOptions,
): UseMutationResult<string, Error, ToolId> {
  return useLifecycleMutation<ToolId>(
    (tool) => native.tools.repair(tool),
    options,
  );
}

export function useUninstallTool(
  options?: LifecycleMutationOptions,
): UseMutationResult<string, Error, UninstallVariables> {
  return useLifecycleMutation<UninstallVariables>(
    ({ tool, options: uninstallOptions }) =>
      native.tools.uninstall(tool, uninstallOptions),
    options,
  );
}

export function useCancelOperation(
  options: LifecycleMutationOptions = {},
): UseMutationResult<Operation, Error, string> {
  const queryClient = useQueryClient();
  const { t } = useTranslation();

  return useMutation<Operation, Error, string>({
    mutationFn: (operationId) => native.operations.cancel(operationId),
    onSuccess: (operation) => {
      queryClient.setQueryData<Operation[]>(operationKeys.list(), (current) =>
        mergeOperation(current ?? [], operation),
      );
    },
    onError: (error) => {
      if (options.notifyOnError === false) return;
      const copy = toErrorCopy(error);
      toast.error(t(copy.messageKey), {
        description: copy.remediationKey ? t(copy.remediationKey) : undefined,
      });
    },
  });
}

/** Open is an immediate terminal handoff and never enters OperationManager; failures still use the same unified human-readable error model. */
export function useLaunchTool(
  options: LaunchMutationOptions = {},
): UseMutationResult<ToolLaunchOutcome, Error, LaunchToolVariables> {
  const { t } = useTranslation();

  return useMutation<ToolLaunchOutcome, Error, LaunchToolVariables>({
    mutationFn: ({ tool, directoryMode }) =>
      native.tools.launch(tool, directoryMode),
    onError: (error) => {
      if (options.notifyOnError === false) return;
      const copy = toErrorCopy(error);
      toast.error(t(copy.messageKey), {
        description: copy.remediationKey ? t(copy.remediationKey) : undefined,
      });
    },
  });
}
