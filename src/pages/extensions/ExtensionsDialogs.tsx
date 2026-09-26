import type { Extension, ExtensionKind } from "@/entities/extension";
import type { Tool } from "@/entities/tool";
import {
  type ExtensionScopeOption,
  McpInstallModal,
  McpRemovalModal,
  PromptEditorModal,
  PromptRemovalModal,
  SkillCatalogModal,
  SkillRemovalModal,
} from "@/features/extension-management";

interface ExtensionsDialogsProps {
  activeKind: ExtensionKind;
  activeScope: ExtensionScopeOption | null;
  activeTool: Tool | null;
  catalogOpen: boolean;
  mcpInstallOpen: boolean;
  promptEditorOpen: boolean;
  editingPrompt: Extension | null;
  mutationsBlocked: boolean;
  removingExtension: Extension | null;
  onCatalogOpenChange: (open: boolean) => void;
  onMcpInstallOpenChange: (open: boolean) => void;
  onPromptEditorOpenChange: (open: boolean) => void;
  onRemovalOpenChange: (open: boolean) => void;
}

export function ExtensionsDialogs({
  activeKind,
  activeScope,
  activeTool,
  catalogOpen,
  mcpInstallOpen,
  promptEditorOpen,
  editingPrompt,
  mutationsBlocked,
  removingExtension,
  onCatalogOpenChange,
  onMcpInstallOpenChange,
  onPromptEditorOpenChange,
  onRemovalOpenChange,
}: ExtensionsDialogsProps) {
  return (
    <>
      {activeKind === "skill" && activeTool ? (
        <SkillCatalogModal
          open={catalogOpen}
          tool={activeTool.id}
          toolName={activeTool.name}
          mutationsBlocked={mutationsBlocked}
          onOpenChange={onCatalogOpenChange}
        />
      ) : null}

      {activeKind === "mcp" && activeScope ? (
        <McpInstallModal
          open={mcpInstallOpen}
          scope={activeScope.scope}
          scopeName={activeScope.name}
          mutationsBlocked={mutationsBlocked}
          onOpenChange={onMcpInstallOpenChange}
        />
      ) : null}

      {activeKind === "prompt" && activeTool ? (
        <>
          <PromptEditorModal
            open={promptEditorOpen}
            tool={activeTool.id}
            toolName={activeTool.name}
            prompt={editingPrompt}
            mutationsBlocked={mutationsBlocked}
            onOpenChange={onPromptEditorOpenChange}
          />
        </>
      ) : null}

      <SkillRemovalModal
        skill={removingExtension?.kind === "skill" ? removingExtension : null}
        mutationsBlocked={mutationsBlocked}
        onOpenChange={onRemovalOpenChange}
      />

      <McpRemovalModal
        connection={
          removingExtension?.kind === "mcp" ? removingExtension : null
        }
        mutationsBlocked={mutationsBlocked}
        onOpenChange={onRemovalOpenChange}
      />

      <PromptRemovalModal
        prompt={removingExtension?.kind === "prompt" ? removingExtension : null}
        mutationsBlocked={mutationsBlocked}
        onOpenChange={onRemovalOpenChange}
      />
    </>
  );
}
