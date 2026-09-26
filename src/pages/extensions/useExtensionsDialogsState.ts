import { useCallback, useState } from "react";
import type { Extension } from "@/entities/extension";

export function useExtensionsDialogsState() {
  const [catalogOpen, setCatalogOpen] = useState(false);
  const [mcpInstallOpen, setMcpInstallOpen] = useState(false);
  const [promptEditorOpen, setPromptEditorOpen] = useState(false);
  const [editingPrompt, setEditingPrompt] = useState<Extension | null>(null);
  const [removingExtension, setRemovingExtension] = useState<Extension | null>(
    null,
  );

  const reset = useCallback(() => {
    setCatalogOpen(false);
    setMcpInstallOpen(false);
    setPromptEditorOpen(false);
    setEditingPrompt(null);
    setRemovingExtension(null);
  }, []);

  return {
    catalogOpen,
    mcpInstallOpen,
    promptEditorOpen,
    editingPrompt,
    removingExtension,
    reset,
    openCatalog: () => setCatalogOpen(true),
    openMcpInstall: () => setMcpInstallOpen(true),
    openPromptCreate: () => {
      setEditingPrompt(null);
      setPromptEditorOpen(true);
    },
    openPromptEdit: (prompt: Extension) => {
      setEditingPrompt(prompt);
      setPromptEditorOpen(true);
    },
    openRemoval: setRemovingExtension,
    setCatalogOpen,
    setMcpInstallOpen,
    setPromptEditorOpen: (open: boolean) => {
      setPromptEditorOpen(open);
      if (!open) setEditingPrompt(null);
    },
    setRemovalOpen: (open: boolean) => {
      if (!open) setRemovingExtension(null);
    },
  };
}
