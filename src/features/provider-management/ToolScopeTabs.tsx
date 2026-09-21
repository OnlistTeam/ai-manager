import { useTranslation } from "react-i18next";
import { hasManageableConfiguration } from "@/entities/tool";
import type { Tool, ToolId } from "@/entities/tool";
import { ScopeTabs } from "@/shared/ui/ScopeTabs";

export interface ToolScopeTabsProps {
  tools: readonly Tool[];
  active: ToolId | null;
  disabled?: boolean;
  /** Accessible name, e.g. "AI Tools". */
  label: string;
  /** Short status shown beside installed tools that cannot use this page. */
  unsupportedLabel?: string;
  onSelect: (tool: ToolId) => void;
}

/**
 * A provider belongs to exactly one tool (the upstream providers table is
 * partitioned by tool), so the page has to ask "which tool's providers"
 * first. Capability controls the status and safe management path, never the
 * tool's visibility and never a hard-coded tool-name branch (AI_RULES rule 8).
 *
 * The tabs' own styling and accessibility semantics live in the design
 * system's `ScopeTabs`; this component only holds the product knowledge —
 * which tools should appear in this row. Every installed tool remains visible;
 * unsupported tools carry a status label and render a read-only explanation
 * in the page instead of disappearing when the scope changes.
 */
export function ToolScopeTabs({
  tools,
  active,
  disabled = false,
  label,
  unsupportedLabel,
  onSelect,
}: ToolScopeTabsProps) {
  const { t } = useTranslation();
  const installed = tools.filter(hasManageableConfiguration);

  /**
   * A tool can be here without its command line, because a desktop application
   * owns the same configuration. Saying so is what stops the row looking like
   * a detection bug to someone who knows the CLI is not installed.
   */
  const statusFor = (tool: Tool) => {
    if (tool.status === "notInstalled" && tool.configurationSharedWith) {
      return t("services.scopeSharedShort");
    }
    if (tool.capabilities.canManageProvider || !unsupportedLabel) {
      return undefined;
    }
    return unsupportedLabel;
  };

  return (
    <ScopeTabs
      items={installed.map((tool) => ({
        id: tool.id,
        label: tool.name,
        statusLabel: statusFor(tool),
      }))}
      active={active}
      disabled={disabled}
      label={label}
      onSelect={(id) => {
        // ScopeTabs is purely presentational and hands back a string. Look
        // it up once in this feature's own list, so no type assertion is
        // needed and it's impossible to pick a tool that isn't in the table.
        const picked = installed.find((tool) => tool.id === id);
        if (picked) onSelect(picked.id);
      }}
    />
  );
}
