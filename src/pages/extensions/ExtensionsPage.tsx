import { ExtensionLocationRow } from "./ExtensionLocationRow";
import { ExtensionsScopedPanel } from "./ExtensionsScopedPanel";
import { useTranslation } from "react-i18next";
import { useDesktopApps } from "@/entities/desktop-app";
import type { ExtensionKind } from "@/entities/extension";
import { scopeTabId } from "@/shared/ui/ScopeTabs";
import { SectionHeader } from "@/shared/ui/SectionHeader";
import { OpenClawWorkspacePage } from "@/pages/workspace";
import { ExtensionsDialogs } from "./ExtensionsDialogs";
import { ExtensionsHeaderActions } from "./ExtensionsHeaderActions";
import {
  ExtensionsKindTabs,
  KIND_PANEL_ID,
  KIND_TAB_PREFIX,
} from "./ExtensionsKindTabs";
import { ExtensionsSkeleton } from "./ExtensionsSkeleton";
import { ExtensionsUnifiedPanel } from "./ExtensionsUnifiedPanel";
import {
  ExtensionsToolRefreshNotice,
  ExtensionsToolsUnavailable,
} from "./ExtensionsToolInventoryNotice";
import { useExtensionScope } from "./useExtensionScope";
import { useExtensionsInventory } from "./useExtensionsInventory";
import { useExtensionsDialogsState } from "./useExtensionsDialogsState";
import { useExtensionsToolRecovery } from "./useExtensionsToolRecovery";
import {
  WORKSPACE_TAB,
  openClawWorkspaceAvailable,
  useExtensionsWorkspaceTab,
  type ExtensionsTab,
} from "./useExtensionsWorkspaceTab";

export interface ExtensionsPageProps {
  fixedKind?: ExtensionKind;
  preferredTab?: ExtensionsTab | null;
  preferredKind?: ExtensionKind | null;
}

export function ExtensionsPage({
  preferredTab = null,
  preferredKind = null,
  fixedKind,
}: ExtensionsPageProps = {}) {
  const { t } = useTranslation();
  const preferredScope =
    preferredTab !== null && preferredTab !== WORKSPACE_TAB
      ? preferredTab
      : null;
  const dialogs = useExtensionsDialogsState();
  const {
    tools,
    pageRef,
    retryButtonRef,
    dataAvailable: toolDataAvailable,
    initiallyLoading: toolsInitiallyLoading,
    unavailable: toolsUnavailable,
    refreshFailed: toolsRefreshFailed,
    actionsBlocked: toolActionsBlocked,
    retryTools,
  } = useExtensionsToolRecovery();
  const desktopApps = useDesktopApps();
  const scope = useExtensionScope(
    tools.data ?? [],
    desktopApps.data ?? [],
    toolDataAvailable,
    preferredScope,
    preferredKind,
    fixedKind,
  );
  const { activeKind, activeTab, activeScope } = scope;
  const workspaceAvailable =
    !fixedKind &&
    toolDataAvailable &&
    openClawWorkspaceAvailable(tools.data ?? []);
  const workspace = useExtensionsWorkspaceTab(
    workspaceAvailable,
    preferredTab === WORKSPACE_TAB,
  );
  const {
    task,
    unifiedLayout,
    target,
    unified,
    extensions,
    kindOperations,
    activeOperation,
    skillUpdates,
    mutationsBlocked,
  } = useExtensionsInventory(scope, preferredScope, toolActionsBlocked);

  return (
    <div
      ref={pageRef}
      role="region"
      aria-label={t(fixedKind ? activeTab.titleKey : "extensions.title")}
      tabIndex={-1}
      className="flex min-w-0 flex-col gap-4 outline-none"
    >
      <SectionHeader
        className="flex-wrap items-center"
        as="h1"
        title={t(fixedKind ? activeTab.titleKey : "extensions.title")}
        action={
          toolDataAvailable &&
          scope.scopedTargets.length > 0 &&
          !workspace.active ? (
            <ExtensionsHeaderActions
              tab={activeTab}
              target={target}
              dialogs={dialogs}
              blocked={mutationsBlocked}
            />
          ) : undefined
        }
      />

      {!fixedKind ? (
        <ExtensionsKindTabs
          active={workspace.active ? WORKSPACE_TAB : activeKind}
          workspaceAvailable={workspaceAvailable}
          disabled={scope.saving || !toolDataAvailable}
          saving={scope.saving}
          saveFailed={scope.saveFailed}
          onSelectKind={(kind) => {
            dialogs.reset();
            workspace.close();
            scope.selectKind(kind);
          }}
          onSelectWorkspace={() => {
            dialogs.reset();
            workspace.open();
          }}
          onRetry={scope.retry}
        />
      ) : null}

      {workspace.active ? (
        <section
          id={KIND_PANEL_ID}
          role="tabpanel"
          aria-labelledby={scopeTabId(KIND_TAB_PREFIX, WORKSPACE_TAB)}
          className="flex min-w-0 flex-col"
        >
          <OpenClawWorkspacePage />
        </section>
      ) : (
        <>
          {toolsInitiallyLoading ? (
            <ExtensionsSkeleton label={t("extensions.loading")} />
          ) : null}

          {toolsUnavailable ? (
            <ExtensionsToolsUnavailable
              refreshing={tools.isFetching}
              retryButtonRef={retryButtonRef}
              onRetry={retryTools}
            />
          ) : null}

          {toolsRefreshFailed ? (
            <ExtensionsToolRefreshNotice
              refreshing={tools.isFetching}
              retryButtonRef={retryButtonRef}
              onRetry={retryTools}
            />
          ) : null}

          {toolDataAvailable && activeKind !== null && unifiedLayout ? (
            <ExtensionsUnifiedPanel
              tab={activeTab}
              fixedKind={Boolean(fixedKind)}
              targets={scope.scopedTargets}
              unified={unified}
              operations={kindOperations}
              task={task}
              skillUpdates={skillUpdates}
              actionsBlocked={toolActionsBlocked}
              dialogs={dialogs}
            />
          ) : null}

          {/* The file this list is written into, named once above the list
              rather than repeated on every card (ADR-0045). */}
          {toolDataAvailable &&
          activeKind !== null &&
          !unifiedLayout &&
          activeScope?.supported ? (
            <ExtensionLocationRow
              key={activeScope.key + activeTab.kind}
              scope={activeScope.scope}
              kind={activeTab.kind}
            />
          ) : null}

          {toolDataAvailable && activeKind !== null && !unifiedLayout ? (
            <ExtensionsScopedPanel
              scope={scope}
              dialogs={dialogs}
              fixedKind={Boolean(fixedKind)}
              activeOperation={activeOperation}
              extensions={extensions}
              task={task}
              inventoryActionsBlocked={toolActionsBlocked}
            />
          ) : null}
        </>
      )}

      <ExtensionsDialogs
        activeKind={activeTab.kind}
        activeScope={target}
        activeTool={target?.tool ?? null}
        catalogOpen={dialogs.catalogOpen}
        mcpInstallOpen={dialogs.mcpInstallOpen}
        editingMcp={dialogs.editingMcp}
        promptEditorOpen={dialogs.promptEditorOpen}
        editingPrompt={dialogs.editingPrompt}
        mutationsBlocked={mutationsBlocked}
        removingExtension={dialogs.removingExtension}
        onCatalogOpenChange={dialogs.setCatalogOpen}
        onMcpInstallOpenChange={dialogs.setMcpInstallOpen}
        onPromptEditorOpenChange={dialogs.setPromptEditorOpen}
        onRemovalOpenChange={dialogs.setRemovalOpen}
      />
    </div>
  );
}
