import { useTranslation } from "react-i18next";
import type { Tool, ToolId } from "@/entities/tool";
import { ScopeTabs } from "@/shared/ui/ScopeTabs";

const ALL_ID = "__all__";

export interface DataScopeTabsProps {
  /** Installed tools; capability status is shown on each tool scope. */
  tools: readonly Tool[];
  /** null means "all". */
  active: ToolId | null;
  onSelect: (tool: ToolId | null) => void;
  showCapabilityStatus?: boolean;
}

/**
 * The row of pills at the top of the "Local Data" page: one selection drives
 * both which tool the session list and the storage panel show. This has
 * different semantics from `ToolScopeTabs` (used on the service endpoints
 * page: single-select, no "all" option), so they can't be shared — forcing a
 * nullable mode in there would pollute its contract on the endpoints page,
 * hence a separate copy here.
 */
export function DataScopeTabs({
  tools,
  active,
  onSelect,
  showCapabilityStatus = true,
}: DataScopeTabsProps) {
  const { t } = useTranslation();

  return (
    <ScopeTabs
      items={[
        { id: ALL_ID, label: t("data.scope.all") },
        ...tools.map((tool) => ({
          id: tool.id,
          label: tool.name,
          statusLabel:
            !showCapabilityStatus || tool.capabilities.canManageProvider
              ? undefined
              : t("data.scope.unsupportedShort"),
        })),
      ]}
      active={active ?? ALL_ID}
      label={t("data.scope.label")}
      onSelect={(id) => {
        if (id === ALL_ID) {
          onSelect(null);
          return;
        }
        // ScopeTabs is purely presentational and hands back a plain string.
        // Looking it up in the installed tools once means no type assertion is
        // needed, and it's impossible to end up selecting a tool that isn't
        // in the table.
        const picked = tools.find((tool) => tool.id === id);
        if (picked) onSelect(picked.id);
      }}
    />
  );
}
