import { useCallback, useState } from "react";
import type { Extension } from "@/entities/extension";

export function useExtensionsDialogsState() {
  const [catalogOpen, setCatalogOpen] = useState(false);
  const [mcpInstallOpen, setMcpInstallOpen] = useState(false);
  const [promptEditorOpen, setPromptEditorOpen] = useState(false);
  const [promptImportOpen, setPromptImportOpen] = useState(false);
  const [editingPrompt, setEditingPrompt] = useState<Extension | null>(null);
  const [removingExtension, setRemovingExtension] = useState<Extension | null>(
    null,
  );
  const [updatingSkill, setUpdatingSkill] = useState<Extension | null>(null);

  const reset = useCallback(() => {
    setCatalogOpen(false);
    setMcpInstallOpen(false);
    setPromptEditorOpen(false);
    setPromptImportOpen(false);
    setEditingPrompt(null);
    setRemovingExtension(null);
    setUpdatingSkill(null);
  }, []);

  return {
    catalogOpen,
    mcpInstallOpen,
    promptEditorOpen,
    promptImportOpen,
    editingPrompt,
    removingExtension,
    updatingSkill,
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
    openPromptImport: () => setPromptImportOpen(true),
    openRemoval: setRemovingExtension,
    openSkillUpdate: setUpdatingSkill,
    setCatalogOpen,
    setMcpInstallOpen,
    setPromptImportOpen,
    setPromptEditorOpen: (open: boolean) => {
      setPromptEditorOpen(open);
      if (!open) setEditingPrompt(null);
    },
    setRemovalOpen: (open: boolean) => {
      if (!open) setRemovingExtension(null);
    },
    setSkillUpdateOpen: (open: boolean) => {
      if (!open) setUpdatingSkill(null);
    },
  };
}
