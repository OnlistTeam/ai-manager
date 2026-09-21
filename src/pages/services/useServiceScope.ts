import { useState } from "react";
import {
  useProductSettings,
  useSaveProductSettings,
} from "@/entities/settings";
import { hasManageableConfiguration } from "@/entities/tool";
import type { Tool, ToolId } from "@/entities/tool";

/**
 * Keeps an explicit page choice usable even when remembering it fails. The
 * settings Query remains the only persisted state; selectedScope is only the
 * current visit's presentation fallback.
 */
export function useServiceScope(
  tools: readonly Tool[],
  preferredToolId: ToolId | null,
) {
  const settings = useProductSettings();
  const save = useSaveProductSettings();
  const [selectedScope, setSelectedScope] = useState<ToolId | null>(
    preferredToolId,
  );
  const installed = tools.filter(hasManageableConfiguration);
  const stored = settings.data?.toolScope ?? null;
  const activeTool =
    installed.find((tool) => tool.id === selectedScope) ??
    installed.find((tool) => tool.id === stored) ??
    installed.find((tool) => tool.capabilities.canManageProvider) ??
    installed[0] ??
    null;
  const active = activeTool?.id ?? null;

  const remember = (id: ToolId) => {
    if (save.isPending) return;
    setSelectedScope(id);
    save.mutate(
      { toolScope: id },
      {
        onSuccess: () => setSelectedScope(null),
      },
    );
  };

  return {
    installed,
    activeSupported: activeTool?.capabilities.canManageProvider ?? false,
    activeTool,
    saving: save.isPending,
    saveFailed: save.isError,
    select: (id: ToolId) => {
      if (id !== active || save.isError) remember(id);
    },
    retry: () => {
      if (active !== null) remember(active);
    },
  };
}
