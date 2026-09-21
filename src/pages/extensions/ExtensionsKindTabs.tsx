import { useTranslation } from "react-i18next";
import type { ExtensionKind } from "@/entities/extension";
import { EXTENSION_TABS } from "@/features/extension-management";
import { ScopeSaveNotice } from "@/features/scope-memory";
import { ScopeTabs, type ScopeTabItem } from "@/shared/ui/ScopeTabs";
import { WORKSPACE_TAB } from "./useExtensionsWorkspaceTab";

export const KIND_TAB_PREFIX = "extensions-kind";
export const KIND_PANEL_ID = "extensions-kind-panel";

interface ExtensionsKindTabsProps {
  active: ExtensionKind | typeof WORKSPACE_TAB | null;
  workspaceAvailable: boolean;
  disabled: boolean;
  saving: boolean;
  saveFailed: boolean;
  onSelectKind: (kind: ExtensionKind) => void;
  onSelectWorkspace: () => void;
  onRetry: () => void;
}

/** Skills / MCP / Prompts plus, while OpenClaw is installed, its workspace. */
export function ExtensionsKindTabs({
  active,
  workspaceAvailable,
  disabled,
  saving,
  saveFailed,
  onSelectKind,
  onSelectWorkspace,
  onRetry,
}: ExtensionsKindTabsProps) {
  const { t } = useTranslation();
  const items: ScopeTabItem[] = EXTENSION_TABS.map((tab) => ({
    id: tab.kind,
    label: t(tab.titleKey),
  }));
  if (workspaceAvailable) {
    items.push({ id: WORKSPACE_TAB, label: t("nav.workspace") });
  }

  return (
    <div className="flex flex-col gap-3">
      <ScopeTabs
        items={items}
        active={active}
        disabled={disabled}
        label={t("extensions.kindScope")}
        idPrefix={KIND_TAB_PREFIX}
        panelId={KIND_PANEL_ID}
        onSelect={(id) => {
          if (id === WORKSPACE_TAB) {
            onSelectWorkspace();
            return;
          }
          const picked = EXTENSION_TABS.find((tab) => tab.kind === id);
          if (picked) onSelectKind(picked.kind);
        }}
      />
      <ScopeSaveNotice
        saving={saving}
        failed={saveFailed}
        savingLabel={t("extensions.scopeSave.saving")}
        errorTitle={t("extensions.scopeSave.errorTitle")}
        errorDescription={t("extensions.scopeSave.errorDescription")}
        retryLabel={t("extensions.scopeSave.retry")}
        retryDisabled={saving}
        onRetry={onRetry}
      />
    </div>
  );
}
