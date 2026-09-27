import { Waypoints } from "lucide-react";
import { useMemo, useState } from "react";
import { useTranslation } from "react-i18next";
import { useProviderConnectivity } from "@/entities/health";
import {
  useProviderConnectionProfile,
  useProviderRuntimeContext,
  useToolLoginStatus,
  type Provider,
} from "@/entities/provider";
import type { ToolId } from "@/entities/tool";
import {
  ProviderAddPage,
  ProviderConnectModal,
  ProviderTestModal,
  ProviderToolLoginModal,
  ShellVariableEditModal,
  useProviderConnectionFlow,
  useRecoverableProviderSwitch,
  useRemoveProvider,
  useShellVariables,
  type ProbeSubject,
} from "@/features/provider-management";
import {
  isToolLaunchable,
  useToolLaunchFlow,
} from "@/features/tool-management";
import { EmptyState } from "@/shared/ui/EmptyState";
import { ServicesAddBreadcrumb } from "./ServicesAddBreadcrumb";
import { ServicesDialogs } from "./ServicesDialogs";
import {
  ServicesEmptyInventory,
  ServicesInventoryStatus,
  ServicesUnsupportedScope,
} from "./ServicesInventoryNotice";
import { ServicesProviderGrid } from "./ServicesProviderGrid";
import { ServicesHeaderActions } from "./ServicesHeaderActions";
import {
  ServicesToolRefreshNotice,
  ServicesToolsUnavailable,
} from "./ServicesToolInventoryNotice";
import { ServicesScopePicker } from "./ServicesScopePicker";
import { ServicesStartAction } from "./ServicesStartAction";
import { useProviderEditFlow } from "./useProviderEditFlow";
import { useProviderInventoryRecovery } from "./useProviderInventoryRecovery";
import { useServiceScope } from "./useServiceScope";
import type { ServicesEndpointsView } from "./useServicesTab";
import { useServicesToolRecovery } from "./useServicesToolRecovery";

export interface ServicesEndpointsPanelProps {
  preferredToolId?: ToolId | null;
  /** The saved list, or the add page behind "Add endpoint" (ADR-0057). */
  view: ServicesEndpointsView;
  onViewChange: (view: ServicesEndpointsView) => void;
}

/** The endpoints tab of the AI Services page: pick a tool, then manage its endpoints. */
export function ServicesEndpointsPanel({
  preferredToolId = null,
  view,
  onViewChange,
}: ServicesEndpointsPanelProps) {
  const { t } = useTranslation();
  const {
    tools,
    pageRef,
    retryButtonRef: toolRetryButtonRef,
    initiallyLoading: toolsInitiallyLoading,
    unavailable: toolsUnavailable,
    refreshFailed: toolsRefreshFailed,
    actionsBlocked: toolActionsBlocked,
    retryTools,
  } = useServicesToolRecovery();
  const [removing, setRemoving] = useState<Provider | null>(null);
  // The test dialog owns the whole check, so the card only has to name its subject.
  const [probing, setProbing] = useState<ProbeSubject | null>(null);
  const [editingVariable, setEditingVariable] = useState<string | null>(null);
  const scope = useServiceScope(tools.data ?? [], preferredToolId);
  const installed = scope.installed;
  const activeTool = scope.activeTool;
  const active = activeTool?.id ?? null;
  const activeSupported = scope.activeSupported;
  const managedActive = activeSupported ? active : null;
  const activeName = activeTool?.name ?? "";
  const {
    providers,
    retryButtonRef: providerRetryButtonRef,
    providerInitiallyLoading,
    providerUnavailable,
    providerRefreshFailed,
    providerActionsBlocked,
    retryProviders,
  } = useProviderInventoryRecovery(managedActive, pageRef);
  const providerData = providers.data;
  const runtime = useProviderRuntimeContext(managedActive);
  const configResource = runtime.data?.resources.find(
    (resource) => resource.kind === "configuration",
  );
  const connection = useProviderConnectionProfile(managedActive);
  const connectivity = useProviderConnectivity();
  const launch = useToolLaunchFlow();
  const switchFlow = useRecoverableProviderSwitch(managedActive, {
    toolName: activeName,
    onOpenTool:
      activeTool && isToolLaunchable(activeTool)
        ? () => launch.openTool(activeTool)
        : undefined,
  });
  const connectionFlow = useProviderConnectionFlow({
    onActivate: switchFlow.switchProvider,
  });
  const editFlow = useProviderEditFlow(managedActive, activeName);
  const remove = useRemoveProvider();
  const busy =
    switchFlow.busy ||
    editFlow.busy ||
    connectionFlow.createBusy ||
    connectionFlow.checkBusy ||
    remove.isPending ||
    scope.saving;
  const authorityActionsBlocked = toolActionsBlocked || providerActionsBlocked;
  const canConnect =
    managedActive !== null && connection.isSuccess && providers.isSuccess;
  const connectionProfileLoading =
    managedActive !== null && connection.isFetching && !connection.isFetched;
  const connectionProfileUnavailable =
    managedActive !== null &&
    !connection.data &&
    (connection.isError || (connection.isFetching && connection.isFetched));
  const effectiveUnavailable = managedActive !== null && runtime.isError;
  // The start-up lines behind this tool's connection. Only looked up when the
  // card that would offer the edit is on screen. Every located line is offered,
  // not just the first: the address and the key live on separate lines, and
  // picking one of them silently left the other uneditable.
  const externalConnection =
    runtime.data?.effectiveConnection?.providerId === null &&
    runtime.data?.effectiveConnection?.endpoint !== null;
  const shellVariables = useShellVariables(
    managedActive,
    Boolean(externalConnection),
  );
  const editableVariables = useMemo(
    () =>
      new Set(
        (shellVariables.data ?? [])
          .filter((entry) => entry.editable)
          .map((entry) => entry.variable),
      ),
    [shellVariables.data],
  );
  // When the shell environment couldn't be inspected, the check below only
  // looked at the config file and may miss an external override.
  const shellNotInspected =
    runtime.data?.effectiveConnection?.shellInspected === false;
  const activeScopeUnsupported = activeTool !== null && !activeSupported;
  const openAdd = () => onViewChange("add");
  const showList = () => {
    connectionFlow.closeConnection();
    onViewChange("endpoints");
  };
  // The add page needs the tool's catalogue; until it is here (or if it cannot
  // load) the list stays, with its own notice for the failure.
  // The tool's own sign-in is an official entry not bound to an account
  // signed in here (ADR-0061); with one listed there is nothing to put back.
  const toolLoginSaved = (providerData ?? []).some(
    (provider) => provider.kind === "official" && !provider.accountBound,
  );
  const addProfile =
    view === "add" && managedActive !== null && providerData !== undefined
      ? (connection.data ?? null)
      : null;
  // The tool that can be signed in from here, on the add page and the list.
  const toolLogin = connection.data?.toolLogin ?? null;
  const loginStatus = useToolLoginStatus(toolLogin ? managedActive : null);
  // The entry that follows the tool's own sign-in points at no account while
  // the tool is signed out, so the list leaves it out unless it is in use or
  // holds a key (ADR-0061). An unknown answer hides nothing.
  const effectiveId = runtime.data?.effectiveConnection?.providerId ?? null;
  const hiddenIds =
    loginStatus.data?.state === "signedOut"
      ? (providerData ?? [])
          .filter(
            (provider) =>
              provider.kind === "official" &&
              !provider.accountBound &&
              !provider.apiKey &&
              !provider.active &&
              provider.id !== effectiveId,
          )
          .map((provider) => provider.id)
      : [];
  const listed = providerData?.filter(
    (provider) => !hiddenIds.includes(provider.id),
  );
  const loginModal = (onSaved?: () => void) =>
    managedActive === null ? null : (
      <ProviderToolLoginModal
        account={connectionFlow.loginOpen ? toolLogin : null}
        tool={managedActive}
        toolName={activeName}
        saved={toolLoginSaved}
        status={loginStatus.data}
        busy={connectionFlow.createBusy}
        error={connectionFlow.createError}
        mutationsBlocked={authorityActionsBlocked && !busy}
        onOpenChange={(open) => {
          if (!open) connectionFlow.closeConnection();
        }}
        onRestore={() =>
          connectionFlow.restoreToolLogin(
            { tool: managedActive, toolName: activeName },
            onSaved,
          )
        }
        onSignedIn={(result) =>
          connectionFlow.signedIn(
            { tool: managedActive, toolName: activeName },
            result,
            onSaved,
          )
        }
      />
    );

  if (addProfile !== null && managedActive !== null) {
    return (
      <div
        ref={pageRef}
        role="region"
        aria-label={t("services.add.title", { tool: activeName })}
        tabIndex={-1}
        className="flex min-w-0 flex-col gap-4 outline-none"
      >
        <ServicesAddBreadcrumb toolName={activeName} onBack={showList} />
        <ProviderAddPage
          tool={managedActive}
          toolName={activeName}
          profile={addProfile}
          loginStatus={loginStatus.data}
          disabled={busy || authorityActionsBlocked}
          onPickPreset={(preset) =>
            connectionFlow.openConnection({ kind: "preset", preset })
          }
          onPickCustom={() => connectionFlow.openConnection({ kind: "custom" })}
          onPickToolLogin={connectionFlow.openToolLogin}
        />
        <ProviderConnectModal
          target={connectionFlow.target}
          profile={addProfile}
          tool={managedActive}
          toolName={activeName}
          busy={connectionFlow.createBusy}
          error={connectionFlow.createError}
          mutationsBlocked={authorityActionsBlocked && !busy}
          onOpenChange={(open) => {
            if (!open) connectionFlow.closeConnection();
          }}
          onErrorReset={connectionFlow.resetCreateError}
          onSubmit={(draft) =>
            connectionFlow.connectProvider(
              { tool: managedActive, toolName: activeName, draft },
              showList,
            )
          }
          onCustomSubmit={(draft) =>
            connectionFlow.connectCustomProvider(
              { tool: managedActive, toolName: activeName, draft },
              showList,
            )
          }
        />
        {loginModal(showList)}
      </div>
    );
  }

  return (
    <div
      ref={pageRef}
      role="region"
      aria-label={t("services.title")}
      tabIndex={-1}
      className="flex min-w-0 flex-col gap-4 outline-none"
    >
      {toolsUnavailable ? (
        <ServicesToolsUnavailable
          refreshing={tools.isFetching}
          retryButtonRef={toolRetryButtonRef}
          onRetry={retryTools}
        />
      ) : null}

      {toolsRefreshFailed ? (
        <ServicesToolRefreshNotice
          refreshing={tools.isFetching}
          retryButtonRef={toolRetryButtonRef}
          onRetry={retryTools}
        />
      ) : null}

      {tools.data !== undefined && installed.length === 0 ? (
        <EmptyState
          icon={Waypoints}
          title={t("services.noTools.title")}
          description={t("services.noTools.description")}
        />
      ) : null}

      {installed.length > 0 ? (
        <ServicesScopePicker
          tools={installed}
          active={active}
          activeName={activeName}
          disabled={busy}
          saving={scope.saving}
          saveFailed={scope.saveFailed}
          onSelect={(id) => {
            connectionFlow.closeConnection();
            onViewChange("endpoints");
            editFlow.close();
            setRemoving(null);
            remove.reset();
            scope.select(id);
          }}
          onRetry={scope.retry}
        />
      ) : null}

      {activeScopeUnsupported ? (
        <ServicesUnsupportedScope toolName={activeName} />
      ) : (
        <ServicesInventoryStatus
          initiallyLoading={toolsInitiallyLoading || providerInitiallyLoading}
          unavailable={providerUnavailable}
          refreshFailed={providerRefreshFailed}
          providerRefreshing={providers.isFetching}
          connectionUnavailable={
            providerData !== undefined && connectionProfileUnavailable
          }
          connectionRefreshing={connection.isFetching}
          effectiveUnavailable={effectiveUnavailable}
          effectiveRefreshing={runtime.isFetching}
          shellNotInspected={shellNotInspected}
          retryButtonRef={providerRetryButtonRef}
          onProviderRetry={retryProviders}
          onConnectionRetry={() => void connection.refetch()}
          onEffectiveRetry={() => void runtime.refetch()}
        />
      )}

      {!activeScopeUnsupported &&
      listed !== undefined &&
      listed.length === 0 ? (
        <ServicesEmptyInventory />
      ) : null}

      {!activeScopeUnsupported &&
      listed !== undefined &&
      listed.length === 0 &&
      managedActive !== null ? (
        <ServicesHeaderActions
          canConnect={canConnect}
          busy={busy}
          actionsBlocked={authorityActionsBlocked}
          connecting={connectionProfileLoading}
          onConnect={openAdd}
        />
      ) : null}

      {!activeScopeUnsupported &&
      listed !== undefined &&
      listed.length > 0 &&
      managedActive !== null ? (
        <div className="flex flex-col gap-3">
          <div className="flex flex-wrap items-end justify-between gap-3">
            <div>
              <h2 className="text-heading text-content">
                {t("services.endpoints.title", { tool: activeName })}
              </h2>
              <p className="mt-0.5 text-caption text-content-muted">
                {t("services.endpoints.count", { count: listed.length })}
              </p>
            </div>
            <div className="flex flex-wrap items-center justify-end gap-2">
              {activeTool ? (
                <ServicesStartAction
                  tool={activeTool}
                  launch={launch}
                  actionsBlocked={authorityActionsBlocked}
                />
              ) : null}
              <ServicesHeaderActions
                canConnect={canConnect}
                busy={busy}
                actionsBlocked={authorityActionsBlocked}
                connecting={connectionProfileLoading}
                onConnect={openAdd}
              />
            </div>
          </div>
          <ServicesProviderGrid
            providers={listed}
            hiddenIds={hiddenIds}
            tool={managedActive}
            toolName={activeName}
            effective={
              runtime.isError ? undefined : runtime.data?.effectiveConnection
            }
            configResource={configResource}
            connectivity={connectivity.data ?? {}}
            busy={busy || authorityActionsBlocked}
            testingProviderId={connectionFlow.testingProviderId}
            switchingProviderId={switchFlow.switchingProviderId}
            recoveringProviderId={switchFlow.recoveringProviderId}
            recoveryUnavailableProviderId={
              switchFlow.recoveryUnavailableProviderId
            }
            switchFailure={switchFlow.switchFailure}
            checkFailure={connectionFlow.checkFailure ?? undefined}
            onUse={switchFlow.switchProvider}
            onTryNext={switchFlow.tryNextHealthy}
            onTest={(providerId) => {
              const provider = listed.find((entry) => entry.id === providerId);
              setProbing(provider ? { kind: "provider", provider } : null);
            }}
            onTestExternal={
              managedActive === null
                ? undefined
                : () =>
                    setProbing({
                      kind: "effective",
                      tool: managedActive,
                      name: activeName,
                    })
            }
            onEditExternalVariable={setEditingVariable}
            externalEditableVariables={editableVariables}
            onBrowseCompatible={canConnect ? openAdd : undefined}
            onEdit={editFlow.open}
            onSignIn={
              toolLogin && canConnect ? connectionFlow.openToolLogin : undefined
            }
            onRemove={(provider) => {
              remove.reset();
              setRemoving(provider);
            }}
          />
        </div>
      ) : null}

      <ProviderTestModal
        subject={probing}
        mutationsBlocked={busy || authorityActionsBlocked}
        onOpenChange={(open) => {
          if (!open) setProbing(null);
        }}
      />

      <ShellVariableEditModal
        location={
          shellVariables.data?.find(
            (entry) => entry.variable === editingVariable,
          ) ?? null
        }
        tool={managedActive}
        onOpenChange={(open) => {
          if (!open) setEditingVariable(null);
        }}
      />

      <ServicesDialogs
        editing={editFlow.provider}
        editingProfile={editFlow.profile.data ?? null}
        editingProfileLoading={
          editFlow.profile.isFetching && !editFlow.profile.isFetched
        }
        editingProfileError={editFlow.profile.error}
        removing={removing}
        toolName={activeName}
        saveBusy={editFlow.busy}
        saveError={editFlow.error}
        removeBusy={remove.isPending}
        removeError={remove.error}
        mutationsBlocked={authorityActionsBlocked && !busy}
        onEditingOpenChange={editFlow.setOpen}
        onEditingErrorReset={editFlow.resetError}
        onRemovalOpenChange={(open) => {
          if (!open) {
            setRemoving(null);
            remove.reset();
          }
        }}
        onSave={editFlow.submit}
        onRemove={() => {
          if (managedActive === null || removing === null) return;
          remove.mutate(
            {
              tool: managedActive,
              providerId: removing.id,
              name: removing.name,
            },
            { onSuccess: () => setRemoving(null) },
          );
        }}
      />
      {loginModal()}
    </div>
  );
}
