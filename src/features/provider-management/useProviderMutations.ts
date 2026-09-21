import {
  useMutation,
  useQueryClient,
  type UseMutationResult,
} from "@tanstack/react-query";
import { useTranslation } from "react-i18next";
import { toast } from "sonner";
import {
  clearProviderConnectivity,
  clearProviderConnectivityBatch,
  healthKeys,
  useProviderConnectivity,
  writeProviderConnectivity,
} from "@/entities/health";
import { isTerminal, useCachedOperations } from "@/entities/operation";
import {
  providerKeys,
  type Provider,
  type ProviderCreateDraft,
  type ProviderCreateResult,
  type ProviderCustomCreateDraft,
  type ProviderDraft,
  type ProviderTestResult,
} from "@/entities/provider";
import { native, type ToolId } from "@/native";
import { toErrorCopy } from "@/shared/lib/nativeError";

export interface ProviderTarget {
  tool: ToolId;
  providerId: string;
}

interface ProviderScope {
  tool: ToolId;
}

interface ProviderMutationPolicy {
  setupMayChange?: boolean;
  inlineError?: boolean;
  providerSettingsMayChange?: boolean;
}

export interface CreateProviderVariables extends ProviderScope {
  requestId: string;
  draft: ProviderCreateDraft;
}

export interface CreateCustomProviderVariables extends ProviderScope {
  requestId: string;
  draft: ProviderCustomCreateDraft;
}

export interface SaveProviderVariables extends ProviderTarget {
  draft: ProviderDraft;
}

export interface ProviderTestMutationOptions {
  /** A caller with durable card feedback can suppress the transient toast. */
  notifyOnError?: boolean;
}

interface ProviderTestAllVariables {
  tool: ToolId;
  providerIds: string[];
}

/**
 * This is **not the same thing** as tool-management's `useLifecycleMutation`:
 * there, the success value is an operation id and the result is driven by
 * events; here, the write completes synchronously and the backend returns
 * the refreshed list directly. The only thing they share is failure
 * presentation — §42's error-decomposition point is nativeError, and only
 * nativeError.
 */
function useProviderMutation<TVariables extends ProviderScope, TData>(
  run: (variables: TVariables) => Promise<TData>,
  onDone?: (data: TData, variables: TVariables) => void,
  policy: ProviderMutationPolicy = {},
): UseMutationResult<TData, Error, TVariables> {
  const { t } = useTranslation();
  const queryClient = useQueryClient();
  return useMutation<TData, Error, TVariables>({
    mutationFn: run,
    onSuccess: onDone,
    // `switch`/`save` are multi-step operations upstream (writing the live
    // config + DB is_current + proxy takeover); if one fails partway
    // through, the cached active badge can drift out of sync with the real
    // state. Invalidating this tool's list forces the next read to go back
    // to the source, which is safer than continuing to trust a cache that
    // may be stale.
    onError: async (error, variables) => {
      const refreshes = [
        queryClient.invalidateQueries({
          queryKey: providerKeys.list(variables.tool),
        }),
      ];
      if (policy.setupMayChange) {
        refreshes.push(
          queryClient.invalidateQueries({ queryKey: healthKeys.snapshots }),
        );
      }
      if (policy.providerSettingsMayChange) {
        refreshes.push(
          queryClient.invalidateQueries({
            queryKey: providerKeys.editProfiles(variables.tool),
          }),
        );
      }
      const authoritativeRefresh = Promise.all(refreshes);
      if (policy.inlineError) {
        await authoritativeRefresh;
        return;
      }
      void authoritativeRefresh;
      const copy = toErrorCopy(error);
      toast.error(t(copy.messageKey), {
        description: copy.remediationKey ? t(copy.remediationKey) : undefined,
      });
    },
  });
}

export function useCreateProvider(): UseMutationResult<
  ProviderCreateResult,
  Error,
  CreateProviderVariables
> {
  const queryClient = useQueryClient();
  return useProviderMutation<CreateProviderVariables, ProviderCreateResult>(
    ({ tool, requestId, draft }) =>
      native.providers.create(tool, requestId, draft),
    (data, { tool }) => {
      queryClient.setQueryData(providerKeys.list(tool), data.providers);
      void queryClient.invalidateQueries({ queryKey: healthKeys.snapshots });
      void queryClient.invalidateQueries({
        queryKey: providerKeys.runtimeContext(tool),
      });
    },
    { setupMayChange: true, inlineError: true },
  );
}

export function useCreateCustomProvider(): UseMutationResult<
  ProviderCreateResult,
  Error,
  CreateCustomProviderVariables
> {
  const queryClient = useQueryClient();
  return useProviderMutation<
    CreateCustomProviderVariables,
    ProviderCreateResult
  >(
    ({ tool, requestId, draft }) =>
      native.providers.createCustom(tool, requestId, draft),
    (data, { tool }) => {
      queryClient.setQueryData(providerKeys.list(tool), data.providers);
      void queryClient.invalidateQueries({ queryKey: healthKeys.snapshots });
      void queryClient.invalidateQueries({
        queryKey: providerKeys.runtimeContext(tool),
      });
    },
    { setupMayChange: true, inlineError: true },
  );
}

export function useSwitchProvider(): UseMutationResult<
  Provider[],
  Error,
  ProviderTarget
> {
  const queryClient = useQueryClient();
  return useProviderMutation<ProviderTarget, Provider[]>(
    ({ tool, providerId }) => native.providers.switch(tool, providerId),
    (data, { tool }) => {
      queryClient.setQueryData(providerKeys.list(tool), data);
      void queryClient.invalidateQueries({ queryKey: healthKeys.snapshots });
      void queryClient.invalidateQueries({
        queryKey: providerKeys.runtimeContext(tool),
      });
    },
    { setupMayChange: true, inlineError: true },
  );
}

export function useSaveProvider(): UseMutationResult<
  Provider[],
  Error,
  SaveProviderVariables
> {
  const queryClient = useQueryClient();
  return useProviderMutation<SaveProviderVariables, Provider[]>(
    ({ tool, providerId, draft }) =>
      native.providers.save(tool, providerId, draft),
    (data, { tool, providerId }) => {
      queryClient.setQueryData(providerKeys.list(tool), data);
      void queryClient.invalidateQueries({
        queryKey: providerKeys.editProfile(tool, providerId),
      });
      clearProviderConnectivity(queryClient, tool, providerId);
      void queryClient.invalidateQueries({ queryKey: healthKeys.snapshots });
      void queryClient.invalidateQueries({
        queryKey: providerKeys.runtimeContext(tool),
      });
    },
    {
      setupMayChange: true,
      inlineError: true,
      providerSettingsMayChange: true,
    },
  );
}

export function useTestProvider(
  options: ProviderTestMutationOptions = {},
): UseMutationResult<ProviderTestResult, Error, ProviderTarget> {
  const { t } = useTranslation();
  const queryClient = useQueryClient();
  // Keep the process-local cache alive even when Quick Check is not currently mounted.
  useProviderConnectivity();
  return useMutation<ProviderTestResult, Error, ProviderTarget>({
    mutationFn: ({ tool, providerId }) =>
      native.providers.test(tool, providerId),
    // A new check makes the previous result stale immediately. Never leave a
    // green address badge visible while its replacement attempt is pending or failed.
    onMutate: ({ tool, providerId }) =>
      clearProviderConnectivity(queryClient, tool, providerId),
    onSuccess: (result, { tool, providerId }) => {
      writeProviderConnectivity(queryClient, tool, providerId, result);
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

/** Starts one native-owned batch while the shared event stream supplies
 * progress and typed partial results. */
export function useTestAllProviders(
  tool: ToolId | null,
  providers: readonly Provider[],
) {
  const { t } = useTranslation();
  const queryClient = useQueryClient();
  const operations = useCachedOperations();
  useProviderConnectivity();
  const providerIds = providers
    .filter((provider) => provider.testable)
    .map((provider) => provider.id);
  const mutation = useMutation<string, Error, ProviderTestAllVariables>({
    mutationFn: ({ tool: target }) => native.providers.testAll(target),
    onMutate: ({ tool: target, providerIds: targets }) =>
      clearProviderConnectivityBatch(queryClient, target, targets),
    onError: (error) => {
      const copy = toErrorCopy(error);
      toast.error(t(copy.messageKey), {
        description: copy.remediationKey ? t(copy.remediationKey) : undefined,
      });
    },
  });
  const operationRunning = operations.some(
    (operation) =>
      operation.kind === "testProviders" &&
      operation.tool === tool &&
      !isTerminal(operation.status),
  );
  const requestStarting =
    mutation.isPending && mutation.variables?.tool === tool;
  const isRunning = requestStarting || operationRunning;

  return {
    canStart: tool !== null && providerIds.length > 0 && !isRunning,
    isRunning,
    start: () => {
      if (tool !== null && providerIds.length > 0 && !isRunning) {
        mutation.mutate({ tool, providerIds });
      }
    },
  };
}
