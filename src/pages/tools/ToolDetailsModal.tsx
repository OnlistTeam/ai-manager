import { useState } from "react";
import { useTranslation } from "react-i18next";
import {
  toolExtensionScope,
  type ExtensionKind,
  type ExtensionScope,
} from "@/entities/extension";
import type { Tool } from "@/entities/tool";
import { ProviderRuntimeContextPanel } from "@/pages/data/ProviderRuntimeContextPanel";
import { openClawWorkspaceAvailable } from "@/pages/extensions/useExtensionsWorkspaceTab";
import { OpenClawWorkspacePage } from "@/pages/workspace";
import { Modal } from "@/shared/ui/Modal";
import { ScopeTabs } from "@/shared/ui/ScopeTabs";

export function ToolDetailsModal({
  tool,
  onClose,
  onOpenExtensions,
  initialWorkspace = false,
}: {
  tool: Tool;
  onClose: () => void;
  initialWorkspace?: boolean;
  onOpenExtensions?: (scope?: ExtensionScope, kind?: ExtensionKind) => void;
}) {
  const { t } = useTranslation();
  const [tab, setTab] = useState(initialWorkspace ? "workspace" : "resources");
  const workspace = openClawWorkspaceAvailable([tool]);
  return (
    <Modal
      open
      onOpenChange={(open) => {
        if (!open) onClose();
      }}
      title={t("tools.details.title", { name: tool.name })}
      description={t("tools.details.description")}
      size="lg"
    >
      <div className="flex flex-col gap-4">
        {workspace ? (
          <ScopeTabs
            items={[
              { id: "resources", label: t("data.storage.title") },
              { id: "workspace", label: t("nav.workspace") },
            ]}
            active={tab}
            onSelect={setTab}
            label={t("tools.details.title", { name: tool.name })}
          />
        ) : null}
        {workspace && tab === "workspace" ? (
          <OpenClawWorkspacePage />
        ) : tool.capabilities.canManageProvider ? (
          <ProviderRuntimeContextPanel
            tool={tool.id}
            toolName={tool.name}
            includeConfiguration
            embedded
            onManageInstructions={
              tool.capabilities.canManagePrompts && onOpenExtensions
                ? () => {
                    onClose();
                    onOpenExtensions(toolExtensionScope(tool.id), "prompt");
                  }
                : null
            }
          />
        ) : (
          <p className="text-body text-content-muted">
            {t("data.scope.unsupportedDescription", { tool: tool.name })}
          </p>
        )}
      </div>
    </Modal>
  );
}
