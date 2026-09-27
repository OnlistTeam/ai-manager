import {
  useProviderEditProfile,
  useProviderRuntimeContext,
  useProviders,
  useToolModelChoice,
} from "@/entities/provider";
import type { Tool } from "@/entities/tool";
import {
  useRecoverableProviderSwitch,
  useSetToolEffort,
  useSetToolModel,
} from "@/features/provider-management";
import {
  inUseProviderId,
  pickerProviders,
  pinnedModel,
  toolConnection,
  type ToolConnection,
} from "./homeToolConnection";

/**
 * One tool row's connection plus the same switch flow the API Endpoints page
 * uses, so a switch from Home runs the same preflight, announces the reopen
 * hint and leaves the same recovery state.
 *
 * The row answers from the saved inventory at once. The effective connection
 * comes from the same session-cached query the endpoints page reads, so the
 * slow login-shell probe runs once for both pages; until a fresh answer is in
 * (first load, or a re-read after a switch) the saved answer stands, and a
 * failed read never promotes stale evidence (ADR-0039).
 *
 * Where the tool allows it, the row also chooses the model and the thinking
 * effort (ADR-0055); the model shown is the one in the tool's file.
 */
export function useHomeToolConnection(tool: Tool, onOpenTool?: () => void) {
  const providers = useProviders(tool.id);
  const runtime = useProviderRuntimeContext(tool.id);
  const effective =
    runtime.isSuccess && !runtime.isFetching
      ? runtime.data.effectiveConnection
      : null;
  const connection: ToolConnection = toolConnection(
    tool,
    providers.data,
    providers.isError,
    effective,
  );
  const inUseId = inUseProviderId(connection);
  const profile = useProviderEditProfile(inUseId ? tool.id : null, inUseId);
  const modelChoice = useToolModelChoice(
    tool.capabilities.canChooseModel ? tool.id : null,
  );
  const switchFlow = useRecoverableProviderSwitch(tool.id, {
    toolName: tool.name,
    onOpenTool,
  });
  const setModel = useSetToolModel();
  const setEffort = useSetToolEffort();
  const providerName = (id: string | undefined) =>
    providers.data?.find((provider) => provider.id === id)?.name ?? null;
  const choice = modelChoice.data;

  /**
   * A model picked under another endpoint switches to that endpoint first and
   * is set only once the switch has succeeded, so a silent target still
   * changes nothing.
   */
  function chooseModel(providerId: string | null, model: string | null) {
    const set = () => setModel.mutate({ tool: tool.id, providerId, model });
    if (providerId !== null && providerId !== inUseId) {
      switchFlow.switchProvider(providerId, set);
      return;
    }
    set();
  }

  return {
    connection,
    inUseId,
    model: choice ? choice.model : inUseId ? pinnedModel(profile.data) : null,
    choice,
    choiceUnavailable: modelChoice.isError,
    choices: pickerProviders(providers.data, inUseId),
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
    switchProvider: (providerId: string) =>
      switchFlow.switchProvider(providerId),
    chooseModel,
    settingModel: setModel.isPending,
    chooseEffort: (effort: string | null) =>
      setEffort.mutate({ tool: tool.id, effort }),
    settingEffort: setEffort.isPending,
  };
}

export type HomeToolConnection = ReturnType<typeof useHomeToolConnection>;
