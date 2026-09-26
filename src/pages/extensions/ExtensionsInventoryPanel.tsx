import { Puzzle } from "lucide-react";
import { useTranslation } from "react-i18next";
import {
  useExtensions,
  type Extension,
  type ExtensionScope,
} from "@/entities/extension";
import type { Operation } from "@/entities/operation";
import { type ExtensionTab } from "@/features/extension-management";
import {
  TaskAvailabilityNotice,
  useTaskAvailability,
} from "@/features/task-center";
import { EmptyState } from "@/shared/ui/EmptyState";
import { Button } from "@/shared/ui/Button";
import { ExtensionsGrid } from "./ExtensionsGrid";
import { ExtensionsRefreshNotice } from "./ExtensionsRefreshNotice";
import { ExtensionsSkeleton } from "./ExtensionsSkeleton";

interface ExtensionsInventoryPanelProps {
  activeOperation?: Operation;
  extensions: ReturnType<typeof useExtensions>;
  mutationsBlocked: boolean;
  tab: ExtensionTab;
  task: ReturnType<typeof useTaskAvailability>;
  scope: ExtensionScope;
  scopeName: string;
  onEdit?: (extension: Extension) => void;
  onRemove?: (extension: Extension) => void;
}

export function ExtensionsInventoryPanel({
  activeOperation,
  extensions,
  mutationsBlocked,
  tab,
  task,
  scope,
  scopeName,
  onEdit,
  onRemove,
}: ExtensionsInventoryPanelProps) {
  const { t } = useTranslation();
  const dataAvailable = extensions.data !== undefined;
  const initiallyLoading = extensions.isPending && !extensions.isFetched;
  const unavailable = extensions.isFetched && !dataAvailable;
  const refreshFailed = extensions.isError && dataAvailable;
  const actionsBlocked =
    mutationsBlocked || extensions.isFetching || extensions.isError;

  return (
    <>
      <TaskAvailabilityNotice
        state={task.state}
        refreshing={task.operations.isFetching}
        onRetry={() => void task.operations.refetch()}
      />

      {initiallyLoading ? (
        <ExtensionsSkeleton label={t("extensions.loading")} />
      ) : null}

      {unavailable ? (
        <div
          role="alert"
          aria-label={t("extensions.error.title")}
          aria-busy={extensions.isFetching || undefined}
        >
          <EmptyState
            icon={Puzzle}
            title={t("extensions.error.title")}
            description={t("extensions.error.description")}
            action={
              <Button
                loading={extensions.isFetching}
                onClick={() => void extensions.refetch()}
              >
                {t("extensions.refresh")}
              </Button>
            }
          />
        </div>
      ) : null}

      {refreshFailed ? (
        <ExtensionsRefreshNotice
          refreshing={extensions.isFetching}
          onRetry={() => void extensions.refetch()}
        />
      ) : null}

      {dataAvailable ? (
        extensions.data.length === 0 ? (
          <EmptyState
            icon={Puzzle}
            title={t(tab.emptyTitleKey)}
            description={t(tab.emptyDescriptionKey)}
          />
        ) : (
          <ExtensionsGrid
            scope={scope}
            scopeName={scopeName}
            kind={tab.kind}
            extensions={extensions.data}
            activeOperation={activeOperation}
            actionsBlocked={actionsBlocked || task.actionsBlocked}
            onEdit={onEdit}
            onRemove={onRemove}
          />
        )
      ) : null}
    </>
  );
}
