import { PackageSearch } from "lucide-react";
import { useTranslation } from "react-i18next";
import { EmptyState } from "@/shared/ui/EmptyState";
import { SectionHeader } from "@/shared/ui/SectionHeader";
import {
  ToolsInventoryNotice,
  ToolsRefreshButton,
  ToolsUnavailableState,
} from "./ToolsInventoryNotice";
import { ToolsSkeleton } from "./ToolsSkeleton";
import type { useToolInventoryRecovery } from "./useToolInventoryRecovery";

type Recovery = ReturnType<typeof useToolInventoryRecovery>;
export function ToolsPageInventory({
  tools,
  inventoryInitiallyLoading,
  inventoryUnavailable,
  inventoryRefreshFailed,
  retryInventory,
}: Pick<
  Recovery,
  | "tools"
  | "inventoryInitiallyLoading"
  | "inventoryUnavailable"
  | "inventoryRefreshFailed"
  | "retryInventory"
>) {
  const { t } = useTranslation();
  return (
    <>
      <SectionHeader
        as="h1"
        title={t("tools.title")}
        description={t("tools.description")}
        action={
          // The error state already carries its own retry button with the
          // same label (see EmptyState below), so we step aside here —
          // otherwise a screen reader would announce two identical
          // "Check again" buttons.
          tools.isError ? undefined : (
            <ToolsRefreshButton
              refreshing={tools.isFetching}
              onRefresh={() => void tools.refetch()}
            />
          )
        }
      />

      {inventoryInitiallyLoading ? (
        <ToolsSkeleton label={t("tools.loading")} />
      ) : null}

      {inventoryUnavailable ? (
        <ToolsUnavailableState
          refreshing={tools.isFetching}
          onRetry={retryInventory}
        />
      ) : null}

      {inventoryRefreshFailed ? (
        <ToolsInventoryNotice
          refreshing={tools.isFetching}
          onRetry={retryInventory}
        />
      ) : null}

      {tools.data?.length === 0 ? (
        <EmptyState
          icon={PackageSearch}
          title={t("tools.empty.title")}
          description={t("tools.empty.description")}
        />
      ) : null}
    </>
  );
}
