import { Puzzle } from "lucide-react";
import { useTranslation } from "react-i18next";
import type { ComponentProps } from "react";
import { ScopeSaveNotice } from "@/features/scope-memory";
import { EmptyState } from "@/shared/ui/EmptyState";
import { ScopeTabs, scopeTabId } from "@/shared/ui/ScopeTabs";
import { ExtensionsInventoryPanel } from "./ExtensionsInventoryPanel";
import { KIND_PANEL_ID, KIND_TAB_PREFIX } from "./ExtensionsKindTabs";
import type { useExtensionScope } from "./useExtensionScope";
import type { useExtensionsDialogsState } from "./useExtensionsDialogsState";

const TOOL_TAB_PREFIX = "extensions-tool";
const TOOL_PANEL_ID = "extensions-tool-panel";
type InventoryProps = ComponentProps<typeof ExtensionsInventoryPanel>;
interface Props {
  scope: ReturnType<typeof useExtensionScope>;
  dialogs: ReturnType<typeof useExtensionsDialogsState>;
  fixedKind: boolean;
  activeOperation: InventoryProps["activeOperation"];
  extensions: InventoryProps["extensions"];
  task: InventoryProps["task"];
  skillUpdates: InventoryProps["skillUpdates"];
  tools: InventoryProps["tools"];
  inventoryActionsBlocked: boolean;
}
export function ExtensionsScopedPanel({
  scope,
  dialogs,
  fixedKind,
  activeOperation,
  extensions,
  task,
  skillUpdates,
  tools,
  inventoryActionsBlocked,
}: Props) {
  const { t } = useTranslation();
  const { activeKind, activeTab, activeScope, scopedTargets } = scope;
  if (!activeKind) return null;
  return (
    <section
      id={KIND_PANEL_ID}
      role={fixedKind ? undefined : "tabpanel"}
      aria-labelledby={
        fixedKind ? undefined : scopeTabId(KIND_TAB_PREFIX, activeKind)
      }
      className="flex flex-col gap-4"
    >
      {scopedTargets.length === 0 ? (
        <EmptyState
          icon={Puzzle}
          title={t("extensions.noTools.title")}
          description={[
            t(activeTab.explainerKey),
            activeTab.noteKey ? t(activeTab.noteKey) : null,
            t("extensions.noTools.description"),
          ]
            .filter(Boolean)
            .join(" ")}
        />
      ) : (
        <>
          <ScopeTabs
            items={scopedTargets.map((item) => ({
              id: item.key,
              label: item.name,
              statusLabel: item.supported
                ? undefined
                : t("extensions.scope.unsupportedShort"),
            }))}
            active={activeScope?.key ?? null}
            disabled={scope.saving}
            label={t("extensions.toolScope")}
            idPrefix={TOOL_TAB_PREFIX}
            panelId={TOOL_PANEL_ID}
            onSelect={(id) => {
              const picked = scopedTargets.find((item) => item.key === id);
              if (picked) {
                dialogs.reset();
                scope.selectTarget(picked.scope);
              }
            }}
          />

          {fixedKind ? (
            <ScopeSaveNotice
              saving={scope.saving}
              failed={scope.saveFailed}
              savingLabel={t("extensions.scopeSave.saving")}
              errorTitle={t("extensions.scopeSave.errorTitle")}
              errorDescription={t("extensions.scopeSave.errorDescription")}
              retryLabel={t("extensions.scopeSave.retry")}
              retryDisabled={scope.saving}
              onRetry={scope.retry}
            />
          ) : null}

          {activeScope ? (
            <section
              id={TOOL_PANEL_ID}
              role="tabpanel"
              aria-labelledby={scopeTabId(TOOL_TAB_PREFIX, activeScope.key)}
              className="flex flex-col gap-4"
            >
              {activeScope.supported ? (
                <>
                  <ExtensionsInventoryPanel
                    activeOperation={activeOperation}
                    extensions={extensions}
                    mutationsBlocked={inventoryActionsBlocked}
                    tab={activeTab}
                    task={task}
                    scope={activeScope.scope}
                    scopeName={activeScope.name}
                    skillUpdates={skillUpdates}
                    tools={tools}
                    copyTargets={scopedTargets.flatMap((target) =>
                      target.tool === null || !target.supported
                        ? []
                        : [target.tool],
                    )}
                    onEdit={
                      activeTab.kind === "prompt"
                        ? dialogs.openPromptEdit
                        : undefined
                    }
                    onRemove={
                      activeTab.kind === "skill" ||
                      activeTab.kind === "mcp" ||
                      activeTab.kind === "prompt"
                        ? dialogs.openRemoval
                        : undefined
                    }
                    onUpdate={dialogs.openSkillUpdate}
                  />
                </>
              ) : (
                <EmptyState
                  icon={Puzzle}
                  title={t("extensions.scope.unsupportedTitle", {
                    tool: activeScope.name,
                    kind: t(activeTab.titleKey),
                  })}
                  description={t("extensions.scope.unsupportedDescription", {
                    tool: activeScope.name,
                    kind: t(activeTab.titleKey),
                  })}
                />
              )}
            </section>
          ) : null}
        </>
      )}
    </section>
  );
}
