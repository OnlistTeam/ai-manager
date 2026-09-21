import { useState } from "react";
import type { Tool, ToolId } from "@/entities/tool";
import { useInstallTool, useRepairTool } from "@/features/tool-management";

export interface PendingAction {
  tool: Tool;
  action: "install" | "repair";
}

export interface ToolInstallFlow {
  /** The install or repair awaiting confirmation; null when closed. */
  pending: PendingAction | null;
  error: Error | null;
  submitting: boolean;
  /** Which tools the in-flight installs and repairs belong to, for the cards' busy state. */
  submittingTools: readonly ToolId[];
  begin: (tool: Tool, action: PendingAction["action"]) => void;
  setOpen: (open: boolean) => void;
  confirm: () => void;
}

/** Install and repair share one confirmation dialog; only the wording differs. */
export function useToolInstallFlow(): ToolInstallFlow {
  const install = useInstallTool({ notifyOnError: false });
  const repair = useRepairTool({ notifyOnError: false });
  const [pending, setPending] = useState<PendingAction | null>(null);
  const submittingTools: ToolId[] = [];
  if (install.isPending && install.variables) {
    submittingTools.push(install.variables);
  }
  if (repair.isPending && repair.variables) {
    submittingTools.push(repair.variables);
  }

  return {
    pending,
    error:
      pending?.action === "install"
        ? install.error
        : pending?.action === "repair"
          ? repair.error
          : null,
    submitting: install.isPending || repair.isPending,
    submittingTools,
    begin: (tool, action) => {
      (action === "install" ? install : repair).reset();
      setPending({ tool, action });
    },
    setOpen: (open) => {
      if (open) return;
      if (pending?.action === "install") install.reset();
      if (pending?.action === "repair") repair.reset();
      setPending(null);
    },
    confirm: () => {
      if (!pending) return;
      const mutation = pending.action === "install" ? install : repair;
      mutation.mutate(pending.tool.id, { onSuccess: () => setPending(null) });
    },
  };
}
