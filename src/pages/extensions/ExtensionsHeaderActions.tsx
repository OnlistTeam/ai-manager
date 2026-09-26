import type {
  ExtensionScopeOption,
  ExtensionTab,
} from "@/features/extension-management";
import { ExtensionsHelp } from "./ExtensionsHelp";
import { ExtensionsPageActions } from "./ExtensionsPageActions";
import type { useExtensionsDialogsState } from "./useExtensionsDialogsState";

export function ExtensionsHeaderActions({
  tab,
  target,
  dialogs,
  blocked,
}: {
  tab: ExtensionTab;
  /** The supported app the add flows start in; null when there is none. */
  target: ExtensionScopeOption | null;
  dialogs: ReturnType<typeof useExtensionsDialogsState>;
  blocked: boolean;
}) {
  const toolTarget = target?.tool ?? null;
  return (
    <div className="flex flex-wrap items-center gap-2">
      <ExtensionsHelp key={tab.kind} tab={tab} />
      {target ? (
        <ExtensionsPageActions
          canAddMcp={tab.kind === "mcp"}
          canAddPrompt={tab.kind === "prompt" && toolTarget !== null}
          canAddSkill={tab.kind === "skill" && toolTarget !== null}
          mutationsBlocked={blocked}
          onAddMcp={dialogs.openMcpInstall}
          onAddPrompt={dialogs.openPromptCreate}
          onAddSkill={dialogs.openCatalog}
        />
      ) : null}
    </div>
  );
}
