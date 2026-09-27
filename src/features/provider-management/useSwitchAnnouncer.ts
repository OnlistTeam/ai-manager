import { useTranslation } from "react-i18next";
import { toast } from "sonner";
import type { Provider, ProviderPreflightOutcome } from "@/entities/provider";
import { routingEndedMessage } from "@/entities/routing";

export interface SwitchAnnouncerOptions {
  /** Named in the reopen hint of every successful switch or failover. */
  toolName: string;
  /** Offers "Open now" on the toast; undefined when the page cannot launch the tool. */
  onOpenTool?: () => void;
}

/**
 * The toast after a successful switch or failover. A switch writes the tool's
 * live configuration; a process that is already running keeps the old
 * endpoint until it is reopened (ADR-0032), so every toast says so and offers
 * to open the tool. When the switch ended the tool's local route because the
 * new endpoint cannot go through AI Manager (ADR-0054), the toast says that
 * instead, with what open sessions do.
 */
export function useSwitchAnnouncer({
  toolName,
  onOpenTool,
}: SwitchAnnouncerOptions) {
  const { t } = useTranslation();

  function providerName(
    providers: readonly Provider[],
    providerId: string | null,
  ) {
    return (
      providers.find((provider) => provider.id === providerId)?.name ??
      t("services.failover.unknownService")
    );
  }

  function successNames(outcome: ProviderPreflightOutcome) {
    return {
      from: providerName(outcome.providers, outcome.originProviderId),
      to: providerName(outcome.providers, outcome.activeProviderId),
    };
  }

  function announce(
    outcome: ProviderPreflightOutcome,
    title: string,
    hint: string,
  ) {
    const description = outcome.routingEnded
      ? routingEndedMessage(t, {
          name: toolName,
          endpoint: providerName(outcome.providers, outcome.activeProviderId),
          pickup: outcome.routingEnded,
        })
      : hint;
    toast.success(title, {
      description,
      action: onOpenTool
        ? { label: t("services.switch.openNow"), onClick: onOpenTool }
        : undefined,
    });
  }

  const reopenHint = () => t("services.switch.reopenHint", { tool: toolName });

  return {
    /** After the Use action switched (or failed over from) `requestedId`. */
    activation(outcome: ProviderPreflightOutcome, requestedId: string) {
      const provider = outcome.providers.find(
        (item) => item.id === requestedId,
      );
      if (provider?.additive) {
        announce(
          outcome,
          t("services.switch.configuredNamed", { name: provider.name }),
          t("services.switch.chooseModelHint", { tool: toolName }),
        );
        return;
      }
      announce(
        outcome,
        outcome.status === "failedOver"
          ? t("services.failover.automaticSuccess", successNames(outcome))
          : t("services.switch.appliedNamed", {
              name: providerName(outcome.providers, outcome.activeProviderId),
            }),
        reopenHint(),
      );
    },
    /** After an explicit Try Next moved the tool to another endpoint. */
    recovery(outcome: ProviderPreflightOutcome) {
      announce(
        outcome,
        t("services.failover.manualSuccess", successNames(outcome)),
        reopenHint(),
      );
    },
  };
}
