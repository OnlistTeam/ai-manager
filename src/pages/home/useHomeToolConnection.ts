import { useProviderEditProfile, useProviders } from "@/entities/provider";
import type { Tool } from "@/entities/tool";
import { useRecoverableProviderSwitch } from "@/features/provider-management";
import {
  pinnedModel,
  switchChoices,
  toolConnection,
  type ToolConnection,
} from "./homeToolConnection";

/**
 * One tool row's cheap connection evidence plus the same switch flow the API
 * Endpoints page uses, so a switch from Home announces the reopen hint and
 * leaves the same recovery state behind.
 */
export function useHomeToolConnection(tool: Tool, onOpenTool?: () => void) {
  const providers = useProviders(tool.id);
  const connection: ToolConnection = toolConnection(
    tool,
    providers.data,
    providers.isError,
  );
  const selected =
    connection.kind === "service" || connection.kind === "official"
      ? (connection.provider ?? null)
      : null;
  const profile = useProviderEditProfile(
    selected ? tool.id : null,
    selected?.id ?? null,
  );
  const switchFlow = useRecoverableProviderSwitch(tool.id, {
    toolName: tool.name,
    onOpenTool,
  });
  const providerName = (id: string | undefined) =>
    providers.data?.find((provider) => provider.id === id)?.name ?? null;

  return {
    connection,
    model: selected ? pinnedModel(profile.data) : null,
    choices: switchChoices(providers.data),
    // A list being re-read, or one that failed to refresh, is not a safe base
    // for choosing what to switch to.
    switchDisabled:
      switchFlow.busy || providers.isFetching || !providers.isSuccess,
    switchingName: providerName(switchFlow.switchingProviderId),
    unreachableName: providerName(switchFlow.unreachableProviderId),
    failure: switchFlow.switchFailure
      ? {
          name: providerName(switchFlow.switchFailure.providerId),
          error: switchFlow.switchFailure.error,
        }
      : null,
    switchProvider: switchFlow.switchProvider,
  };
}

export type HomeToolConnection = ReturnType<typeof useHomeToolConnection>;
