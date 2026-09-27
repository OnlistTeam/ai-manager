import type { ToolId } from "@/entities/tool";
import {
  useNextHealthyProvider,
  useProviderActivationPreflight,
} from "./useProviderPreflight";
import { useSwitchAnnouncer } from "./useSwitchAnnouncer";

export interface ProviderSwitchFailure {
  providerId: string;
  error: Error;
}

export interface RecoverableProviderSwitchOptions {
  /** Named in the reopen hint of every successful switch or failover. */
  toolName: string;
  /**
   * Offers "Open now" on the success toast. Leave undefined when the page
   * cannot launch this tool, so the toast makes no promise it cannot keep.
   */
  onOpenTool?: () => void;
}

export function useRecoverableProviderSwitch(
  tool: ToolId | null,
  { toolName, onOpenTool }: RecoverableProviderSwitchOptions,
) {
  const announcer = useSwitchAnnouncer({ toolName, onOpenTool });
  const activation = useProviderActivationPreflight();
  const recovery = useNextHealthyProvider();
  const scopedVariables =
    activation.variables?.tool === tool ? activation.variables : undefined;
  const scopedRecoveryVariables =
    recovery.variables?.tool === tool ? recovery.variables : undefined;
  const switchFailure: ProviderSwitchFailure | undefined =
    activation.isError && scopedVariables
      ? { providerId: scopedVariables.providerId, error: activation.error }
      : recovery.isError && scopedRecoveryVariables
        ? {
            providerId: scopedRecoveryVariables.failedProviderId,
            error: recovery.error,
          }
        : undefined;
  const switchingProviderId =
    activation.isPending && scopedVariables
      ? scopedVariables.providerId
      : undefined;
  const recoveringProviderId =
    recovery.isPending && scopedRecoveryVariables
      ? scopedRecoveryVariables.failedProviderId
      : undefined;
  // A preflight that found the target silent leaves the tool's connection
  // exactly as it was; the check result itself lands in the connectivity cache.
  const unreachableProviderId =
    activation.isSuccess &&
    scopedVariables &&
    activation.data.status === "unreachable"
      ? scopedVariables.providerId
      : undefined;
  const recoveryUnavailableProviderId =
    recovery.isSuccess &&
    scopedRecoveryVariables &&
    recovery.data.status === "unreachable"
      ? scopedRecoveryVariables.failedProviderId
      : undefined;

  function runActivation(variables: { tool: ToolId; providerId: string }) {
    activation.mutate(variables, {
      onSuccess: (outcome) => {
        if (outcome.status === "unreachable") return;
        announcer.activation(outcome, variables.providerId);
      },
    });
  }

  function switchProvider(providerId: string) {
    if (switchFailure?.providerId === providerId && scopedVariables) {
      runActivation(scopedVariables);
      return;
    }
    if (tool !== null) {
      recovery.reset();
      runActivation({ tool, providerId });
    }
  }

  function tryNextHealthy(providerId: string) {
    if (tool === null) return;
    recovery.mutate(
      { tool, failedProviderId: providerId },
      {
        onSuccess: (outcome) => {
          if (outcome.status !== "failedOver") return;
          announcer.recovery(outcome);
        },
      },
    );
  }

  return {
    busy: activation.isPending || recovery.isPending,
    switchFailure,
    switchingProviderId,
    recoveringProviderId,
    recoveryUnavailableProviderId,
    unreachableProviderId,
    switchProvider,
    tryNextHealthy,
  };
}
