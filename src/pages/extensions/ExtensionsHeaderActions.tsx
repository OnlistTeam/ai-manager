import { ExtensionLocationButton } from "./ExtensionLocationButton";
import { ExtensionsHelp } from "./ExtensionsHelp";
import { ExtensionsPageActions } from "./ExtensionsPageActions";
import type { useExtensionScope } from "./useExtensionScope";
import type { useExtensionsDialogsState } from "./useExtensionsDialogsState";

export function ExtensionsHeaderActions({
  scope,
  dialogs,
  blocked,
}: {
  scope: ReturnType<typeof useExtensionScope>;
  dialogs: ReturnType<typeof useExtensionsDialogsState>;
  blocked: boolean;
}) {
  const { activeTab, activeScope, activeTool } = scope;
  return (
    <div className="flex flex-wrap items-center gap-2">
      <ExtensionsHelp key={activeTab.kind} tab={activeTab} />
      {activeScope?.supported ? (
        <>
          {activeTab.kind !== "skill" ? (
            <ExtensionLocationButton
              key={activeScope.key + activeTab.kind}
              scope={activeScope.scope}
              kind={activeTab.kind}
            />
          ) : null}
          <ExtensionsPageActions
            canAddMcp={activeTab.kind === "mcp"}
            canAddPrompt={activeTab.kind === "prompt" && activeTool !== null}
            canAddSkill={activeTab.kind === "skill" && activeTool !== null}
            mutationsBlocked={blocked}
            onAddMcp={dialogs.openMcpInstall}
            onAddPrompt={dialogs.openPromptCreate}
            onAddSkill={dialogs.openCatalog}
          />
        </>
      ) : null}
    </div>
  );
}
