import type { Provider } from "@/entities/provider";
import type { ToolId } from "@/entities/tool";
import { useTranslation } from "react-i18next";
import { toast } from "sonner";
import {
  useNextHealthyProvider,
  useProviderActivationPreflight,
} from "@/features/provider-management";

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
  const { t } = useTranslation();
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
  const recoveryUnavailableProviderId =
    recovery.isSuccess &&
    scopedRecoveryVariables &&
    recovery.data.status === "unreachable"
      ? scopedRecoveryVariables.failedProviderId
      : undefined;

  function providerName(
    providers: readonly Provider[],
    providerId: string | null,
  ) {
    return (
      providers.find((provider) => provider.id === providerId)?.name ??
      t("services.failover.unknownService")
    );
  }

  function successNames(
    providers: readonly Provider[],
    originProviderId: string | null,
    activeProviderId: string | null,
  ) {
    return {
      from: providerName(providers, originProviderId),
      to: providerName(providers, activeProviderId),
    };
  }

  /**
   * A switch writes the tool's live configuration; a process that is already
   * running keeps the old endpoint until it is reopened (ADR-0032). Every
   * successful switch or failover says so and offers to open the tool.
   */
  function announceSwitch(title: string) {
    toast.success(title, {
      description: t("services.switch.reopenHint", { tool: toolName }),
      action: onOpenTool
        ? { label: t("services.switch.openNow"), onClick: onOpenTool }
        : undefined,
    });
  }

  function runActivation(variables: { tool: ToolId; providerId: string }) {
    activation.mutate(variables, {
      onSuccess: (outcome) => {
        if (outcome.status === "unreachable") return;
        const provider = outcome.providers.find(
          (item) => item.id === variables.providerId,
        );
        if (provider?.additive) {
          toast.success(
            t("services.switch.configuredNamed", { name: provider.name }),
            {
              description: t("services.switch.chooseModelHint", {
                tool: toolName,
              }),
              action: onOpenTool
                ? { label: t("services.switch.openNow"), onClick: onOpenTool }
                : undefined,
            },
          );
          return;
        }
        announceSwitch(
          outcome.status === "failedOver"
            ? t(
                "services.failover.automaticSuccess",
                successNames(
                  outcome.providers,
                  outcome.originProviderId,
                  outcome.activeProviderId,
                ),
              )
            : t("services.switch.appliedNamed", {
                name: providerName(outcome.providers, outcome.activeProviderId),
              }),
        );
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
          announceSwitch(
            t(
              "services.failover.manualSuccess",
              successNames(
                outcome.providers,
                outcome.originProviderId,
                outcome.activeProviderId,
              ),
            ),
          );
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
    switchProvider,
    tryNextHealthy,
  };
}
