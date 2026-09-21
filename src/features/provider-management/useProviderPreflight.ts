import {
  useMutation,
  useQueryClient,
  type QueryClient,
  type UseMutationResult,
} from "@tanstack/react-query";
import {
  clearProviderConnectivity,
  healthKeys,
  writeProviderConnectivity,
} from "@/entities/health";
import { providerKeys, type Provider } from "@/entities/provider";
import { native, type ProviderPreflightOutcome, type ToolId } from "@/native";

export interface ProviderActivationPreflightVariables {
  tool: ToolId;
  providerId: string;
}

export interface ProviderLaunchPreflightVariables {
  tool: ToolId;
}

export interface NextHealthyProviderVariables {
  tool: ToolId;
  failedProviderId: string;
}

const LIGHTWEIGHT_FAILOVER_TOOLS: ReadonlySet<ToolId> = new Set([
  "claude-code",
  "codex",
  "gemini-cli",
  "grok-build",
]);

/** Mirrors the backend's switchable-provider boundary (roadmap R1.9). */
export function supportsLightweightProviderFailover(tool: ToolId): boolean {
  return LIGHTWEIGHT_FAILOVER_TOOLS.has(tool);
}

function writeOutcome(
  queryClient: QueryClient,
  tool: ToolId,
  outcome: ProviderPreflightOutcome,
  setupChanged: boolean,
): void {
  queryClient.setQueryData(providerKeys.list(tool), outcome.providers);
  for (const check of outcome.checks) {
    writeProviderConnectivity(queryClient, tool, check.providerId, check);
  }
  if (outcome.status === "failedOver" && outcome.originProviderId !== null) {
    clearProviderConnectivity(queryClient, tool, outcome.originProviderId);
  }
  if (setupChanged) {
    void queryClient.invalidateQueries({
      queryKey: providerKeys.runtimeContext(tool),
    });
    void queryClient.invalidateQueries({ queryKey: healthKeys.snapshots });
  }
}

function refreshAfterUncertainFailure(
  queryClient: QueryClient,
  tool: ToolId,
): Promise<unknown[]> {
  return Promise.all([
    queryClient.invalidateQueries({ queryKey: providerKeys.list(tool) }),
    queryClient.invalidateQueries({
      queryKey: providerKeys.runtimeContext(tool),
    }),
    queryClient.invalidateQueries({ queryKey: healthKeys.snapshots }),
  ]);
}

/**
 * A Use click performs one backend-owned stream preflight. The UI supplies only
 * the saved provider ID and receives a safe, authoritative provider snapshot.
 */
export function useProviderActivationPreflight(): UseMutationResult<
  ProviderPreflightOutcome,
  Error,
  ProviderActivationPreflightVariables
> {
  const queryClient = useQueryClient();
  return useMutation({
    mutationFn: ({ tool, providerId }) =>
      native.providers.prepareActivation(tool, providerId),
    onMutate: ({ tool, providerId }) =>
      clearProviderConnectivity(queryClient, tool, providerId),
    onSuccess: (outcome, { tool }) => {
      writeOutcome(
        queryClient,
        tool,
        outcome,
        outcome.status !== "unreachable",
      );
    },
    onError: (_error, { tool }) =>
      refreshAfterUncertainFailure(queryClient, tool),
  });
}

/** Checks the active service only after an explicit Open confirmation. */
export function useProviderLaunchPreflight(): UseMutationResult<
  ProviderPreflightOutcome,
  Error,
  ProviderLaunchPreflightVariables
> {
  const queryClient = useQueryClient();
  return useMutation({
    mutationFn: ({ tool }) => native.providers.prepareLaunch(tool),
    onMutate: ({ tool }) => {
      const providers = queryClient.getQueryData<Provider[]>(
        providerKeys.list(tool),
      );
      const active = providers?.find((provider) => provider.active);
      if (active) clearProviderConnectivity(queryClient, tool, active.id);
    },
    onSuccess: (outcome, { tool }) => {
      writeOutcome(queryClient, tool, outcome, outcome.status === "failedOver");
    },
    onError: (_error, { tool }) =>
      refreshAfterUncertainFailure(queryClient, tool),
  });
}

/** Explicit recovery: try saved compatible rows in backend-owned stable order. */
export function useNextHealthyProvider(): UseMutationResult<
  ProviderPreflightOutcome,
  Error,
  NextHealthyProviderVariables
> {
  const queryClient = useQueryClient();
  return useMutation({
    mutationFn: ({ tool, failedProviderId }) =>
      native.providers.tryNextHealthy(tool, failedProviderId),
    // Keep the failed origin visible while alternatives are checked. Clearing it
    // would make the recovery card disappear together with its durable feedback.
    onSuccess: (outcome, { tool }) => {
      writeOutcome(queryClient, tool, outcome, outcome.status === "failedOver");
    },
    onError: (_error, { tool }) =>
      refreshAfterUncertainFailure(queryClient, tool),
  });
}
