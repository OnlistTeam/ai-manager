import { useCallback, useState } from "react";
import type { Extension } from "@/entities/extension";
import type { McpEditTarget } from "@/features/extension-management";

export function useExtensionsDialogsState() {
  const [catalogOpen, setCatalogOpen] = useState(false);
  const [mcpInstallOpen, setMcpInstallOpen] = useState(false);
  const [editingMcp, setEditingMcp] = useState<McpEditTarget | null>(null);
  const [promptEditorOpen, setPromptEditorOpen] = useState(false);
  const [editingPrompt, setEditingPrompt] = useState<Extension | null>(null);
  const [removingExtension, setRemovingExtension] = useState<Extension | null>(
    null,
  );

  const reset = useCallback(() => {
    setCatalogOpen(false);
    setMcpInstallOpen(false);
    setEditingMcp(null);
    setPromptEditorOpen(false);
    setEditingPrompt(null);
    setRemovingExtension(null);
  }, []);

  return {
    catalogOpen,
    mcpInstallOpen,
    editingMcp,
    promptEditorOpen,
    editingPrompt,
    removingExtension,
    reset,
    openCatalog: () => setCatalogOpen(true),
    openMcpInstall: () => {
      setEditingMcp(null);
      setMcpInstallOpen(true);
    },
    /** The same form, prefilled from a saved connection (ADR-0062). */
    openMcpEdit: (target: McpEditTarget) => {
      setEditingMcp(target);
      setMcpInstallOpen(true);
    },
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
    setMcpInstallOpen: (open: boolean) => {
      setMcpInstallOpen(open);
      if (!open) setEditingMcp(null);
    },
    setPromptEditorOpen: (open: boolean) => {
      setPromptEditorOpen(open);
      if (!open) setEditingPrompt(null);
    },
    setRemovalOpen: (open: boolean) => {
      if (!open) setRemovingExtension(null);
    },
  };
}
