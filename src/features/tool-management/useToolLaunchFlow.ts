import { useState } from "react";
import { useTranslation } from "react-i18next";
import { toast } from "sonner";
import type { Provider, ProviderPreflightOutcome } from "@/entities/provider";
import type { Tool } from "@/entities/tool";
import type { ToolLaunchDirectoryMode } from "@/native";
import {
  supportsLightweightProviderFailover,
  useNextHealthyProvider,
  useProviderLaunchPreflight,
} from "@/features/provider-management";
import { useLaunchTool } from "./useToolMutations";

/**
 * One safe launch flow shared by every product entry point. The native command
 * still owns folder selection and terminal hand-off; the UI only holds the
 * selected public Tool record and reports a confirmed launch.
 */
export function useToolLaunchFlow() {
  const { t } = useTranslation();
  const launch = useLaunchTool({ notifyOnError: false });
  const preflight = useProviderLaunchPreflight();
  const recovery = useNextHealthyProvider();
  const [tool, setTool] = useState<Tool | null>(null);
  const [blockedOutcome, setBlockedOutcome] =
    useState<ProviderPreflightOutcome | null>(null);
  const [recoveryUnavailable, setRecoveryUnavailable] = useState(false);
  const [directoryMode, setDirectoryMode] =
    useState<ToolLaunchDirectoryMode>("default");

  const resetAttempts = (): void => {
    launch.reset();
    preflight.reset();
    recovery.reset();
    setBlockedOutcome(null);
    setRecoveryUnavailable(false);
  };

  const openTool = (selected: Tool): void => {
    resetAttempts();
    setTool(selected);
  };

  function providerName(
    providers: readonly Provider[],
    providerId: string | null,
  ): string {
    return (
      providers.find((provider) => provider.id === providerId)?.name ??
      t("services.failover.unknownService")
    );
  }

  const launchSelected = (
    selected: Tool,
    selectedDirectoryMode: ToolLaunchDirectoryMode,
  ): void => {
    launch.mutate(
      { tool: selected.id, directoryMode: selectedDirectoryMode },
      {
        onSuccess: (outcome) => {
          setTool(null);
          setBlockedOutcome(null);
          if (outcome === "launched") {
            toast.success(t("tools.open.launched", { name: selected.name }));
          }
        },
      },
    );
  };

  function continueAfterPreflight(
    selected: Tool,
    outcome: ProviderPreflightOutcome,
    selectedDirectoryMode: ToolLaunchDirectoryMode,
  ): void {
    if (outcome.status === "unreachable") {
      setBlockedOutcome(outcome);
      setRecoveryUnavailable(false);
      return;
    }
    setBlockedOutcome(null);
    if (outcome.status === "failedOver") {
      toast.success(
        t("services.failover.automaticSuccess", {
          from: providerName(outcome.providers, outcome.originProviderId),
          to: providerName(outcome.providers, outcome.activeProviderId),
        }),
      );
    }
    launchSelected(selected, selectedDirectoryMode);
  }

  const confirm = (selectedDirectoryMode: ToolLaunchDirectoryMode): void => {
    if (!tool) return;
    const selected = tool;
    setDirectoryMode(selectedDirectoryMode);
    launch.reset();
    recovery.reset();
    setRecoveryUnavailable(false);
    preflight.mutate(
      { tool: selected.id },
      {
        onSuccess: (outcome) =>
          continueAfterPreflight(selected, outcome, selectedDirectoryMode),
      },
    );
  };

  const tryNextHealthy = (): void => {
    if (!tool || !blockedOutcome?.originProviderId) return;
    const selected = tool;
    launch.reset();
    recovery.mutate(
      {
        tool: selected.id,
        failedProviderId: blockedOutcome.originProviderId,
      },
      {
        onSuccess: (outcome) => {
          if (outcome.status !== "failedOver") {
            setBlockedOutcome(outcome);
            setRecoveryUnavailable(true);
            return;
          }
          setBlockedOutcome(null);
          setRecoveryUnavailable(false);
          toast.success(
            t("services.failover.manualSuccess", {
              from: providerName(outcome.providers, outcome.originProviderId),
              to: providerName(outcome.providers, outcome.activeProviderId),
            }),
          );
          launchSelected(selected, directoryMode);
        },
      },
    );
  };

  const recoveryCandidates = blockedOutcome
    ? blockedOutcome.providers.filter(
        (provider) =>
          provider.id !== blockedOutcome.originProviderId &&
          !provider.active &&
          provider.kind === "custom" &&
          provider.testable,
      )
    : [];
  const providerRecovery =
    blockedOutcome?.status === "unreachable"
      ? {
          providerName: providerName(
            blockedOutcome.providers,
            blockedOutcome.originProviderId,
          ),
          canTryNext:
            tool !== null &&
            blockedOutcome.originProviderId !== null &&
            supportsLightweightProviderFailover(tool.id) &&
            recoveryCandidates.length > 0,
          trying: recovery.isPending,
          unavailable: recoveryUnavailable,
          onTryNext: tryNextHealthy,
        }
      : null;
  const busy = launch.isPending || preflight.isPending || recovery.isPending;

  return {
    tool,
    busy,
    error: launch.error ?? preflight.error ?? recovery.error,
    pendingToolId: busy ? tool?.id : undefined,
    providerRecovery,
    openTool,
    onOpenChange: (open: boolean) => {
      if (!open && !busy) {
        resetAttempts();
        setTool(null);
      }
    },
    confirm,
  };
}
