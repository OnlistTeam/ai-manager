import { useState } from "react";
import { useTranslation } from "react-i18next";
import { useToolInventory, type ToolId } from "@/entities/tool";
import { SessionsPage } from "@/pages/sessions/SessionsPage";
import { SectionHeader } from "@/shared/ui/SectionHeader";
import { InventoryRefreshNotice } from "@/shared/ui/InventoryRefreshNotice";
import { DataScopeTabs } from "./DataScopeTabs";

/** The persisted data route now contains only sessions (ADR-0038). */
export function DataPage() {
  const { t } = useTranslation();
  const tools = useToolInventory();
  const installed = (tools.data ?? []).filter(
    (tool) => tool.status !== "notInstalled",
  );
  const [selectedScope, setScope] = useState<ToolId | null>(null);
  const scope = installed.some((tool) => tool.id === selectedScope)
    ? selectedScope
    : null;
  return (
    <div className="flex flex-col gap-6">
      <SectionHeader
        as="h1"
        title={t("sessions.title")}
        description={t("sessions.description")}
      />
      {tools.isError ? (
        <InventoryRefreshNotice
          title={t("data.inventory.error.title")}
          description={t("sessions.inventoryError")}
          actionLabel={t("data.inventory.retry")}
          refreshing={tools.isFetching}
          onRetry={() => void tools.refetch()}
        />
      ) : null}
      {installed.length > 0 ? (
        <DataScopeTabs
          tools={installed}
          active={scope}
          onSelect={setScope}
          showCapabilityStatus={false}
        />
      ) : null}
      <SessionsPage tool={scope} showHeader={false} />
    </div>
  );
}
