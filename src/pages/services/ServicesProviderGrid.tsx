import { useTranslation } from "react-i18next";
import {
  connectivityFor,
  type ProviderConnectivity,
  type ToolId,
} from "@/entities/health";
import type {
  EffectiveConnection,
  Provider,
  ProviderRuntimeResource,
} from "@/entities/provider";
import {
  ProviderCard,
  supportsLightweightProviderFailover,
  type ProviderCheckFailure,
} from "@/features/provider-management";
import { describeSource } from "./effectiveConnectionCopy";
import { ExternalConnectionCard } from "./ExternalConnectionCard";
import { ServicesOpenConfigAction } from "./ServicesOpenConfigAction";
import {
  overrideSourceFor,
  providerEffectiveState,
  sortByEffectiveState,
} from "./providerEffectiveState";
import type { ProviderSwitchFailure } from "./useRecoverableProviderSwitch";

interface ServicesProviderGridProps {
  providers: readonly Provider[];
  tool: ToolId;
  toolName: string;
  effective: EffectiveConnection | null | undefined;
  /** The config file this tool actually reads; only shown on the card that's currently in effect. */
  configResource?: ProviderRuntimeResource;
  connectivity: ProviderConnectivity;
  busy: boolean;
  testingProviderId?: string;
  switchingProviderId?: string;
  recoveringProviderId?: string;
  recoveryUnavailableProviderId?: string;
  switchFailure?: ProviderSwitchFailure;
  checkFailure?: ProviderCheckFailure;
  onUse: (providerId: string) => void;
  onTest: (providerId: string) => void;
  /** Tests the connection in force when it is not one of the saved services. */
  onTestExternal?: () => void;
  /** Edits the start-up line behind it, when one could be pinned down. */
  onEditExternalVariable?: () => void;
  externalVariableName?: string;
  onTryNext: (providerId: string) => void;
  onBrowseCompatible?: () => void;
  onEdit: (provider: Provider) => void;
  onRemove: (provider: Provider) => void;
}

export function ServicesProviderGrid({
  providers,
  tool,
  toolName,
  effective,
  configResource,
  connectivity,
  busy,
  testingProviderId,
  switchingProviderId,
  recoveringProviderId,
  recoveryUnavailableProviderId,
  switchFailure,
  checkFailure,
  onUse,
  onTest,
  onTestExternal,
  onEditExternalVariable,
  externalVariableName,
  onTryNext,
  onBrowseCompatible,
  onEdit,
  onRemove,
}: ServicesProviderGridProps) {
  const { t } = useTranslation();
  const ordered = sortByEffectiveState(providers, effective);
  const overrideSource = effective ? overrideSourceFor(effective) : null;
  // When the effective address doesn't match any saved endpoint, it becomes the first card in the list.
  const external =
    effective && effective.providerId === null && effective.endpoint !== null
      ? effective
      : null;
  return (
    <div className="grid min-w-0 gap-4">
      {effective?.selection && effective.selection !== "configuration" ? (
        <p className="text-caption text-content-muted" role="status">
          {t(`services.effective.selection.${effective.selection}`, {
            model: effective.model ?? "",
          })}
        </p>
      ) : null}
      {external ? (
        <ExternalConnectionCard
          connection={external}
          toolName={toolName}
          tool={tool}
          configResource={configResource}
          onTest={onTestExternal}
          onEditVariable={onEditExternalVariable}
          editVariableName={externalVariableName}
          actionsBlocked={busy}
        />
      ) : null}
      {ordered.map((provider) => {
        const hasSavedCandidate =
          providers.some(
            (candidate) =>
              candidate.id !== provider.id &&
              !candidate.active &&
              candidate.kind === "custom" &&
              candidate.testable,
          ) && supportsLightweightProviderFailover(tool);
        const state = providerEffectiveState(provider, effective);
        const matched = effective?.providerId === provider.id;
        const sourceNote =
          state === "inUse" &&
          effective &&
          (effective.endpointSource.kind === "shellFile" ||
            effective.endpointSource.kind === "environment")
            ? describeSource(effective.endpointSource, t)
            : null;
        return (
          <ProviderCard
            key={provider.id}
            provider={provider}
            toolName={toolName}
            effectiveState={state}
            effectiveCredential={
              matched
                ? effective?.credential
                : state === "unknown" && !provider.apiKey
                  ? "unknown"
                  : undefined
            }
            configAction={
              matched ? (
                <ServicesOpenConfigAction
                  tool={tool}
                  resource={configResource}
                  actionsBlocked={busy}
                />
              ) : null
            }
            overrideSource={overrideSource}
            sourceNote={sourceNote}
            busy={busy}
            testing={testingProviderId === provider.id}
            switching={switchingProviderId === provider.id}
            tryingNext={recoveringProviderId === provider.id}
            tryNextUnavailable={recoveryUnavailableProviderId === provider.id}
            switchError={
              switchFailure?.providerId === provider.id
                ? switchFailure.error
                : undefined
            }
            testError={
              checkFailure?.tool === tool &&
              checkFailure.providerId === provider.id
                ? checkFailure.error
                : undefined
            }
            testResult={connectivityFor(connectivity, tool, provider.id)}
            onUse={() => onUse(provider.id)}
            onTest={() => onTest(provider.id)}
            onTryNext={
              hasSavedCandidate ? () => onTryNext(provider.id) : undefined
            }
            onBrowseCompatible={onBrowseCompatible}
            onEdit={() => onEdit(provider)}
            onRemove={() => onRemove(provider)}
          />
        );
      })}
    </div>
  );
}
