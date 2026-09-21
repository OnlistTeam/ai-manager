import { Puzzle } from "lucide-react";
import { useMemo } from "react";
import { useTranslation } from "react-i18next";
import {
  useExtensions,
  useLocalExtensionInventory,
  type Extension,
  type ExtensionScope,
} from "@/entities/extension";
import type { Operation } from "@/entities/operation";
import { useSkillUpdates } from "@/entities/skill-update";
import type { Tool } from "@/entities/tool";
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
import { skillOwnersById } from "./localExtensionGroups";
import { SkillUpdatesNotice } from "./SkillUpdatesNotice";

interface ExtensionsInventoryPanelProps {
  activeOperation?: Operation;
  extensions: ReturnType<typeof useExtensions>;
  mutationsBlocked: boolean;
  tab: ExtensionTab;
  task: ReturnType<typeof useTaskAvailability>;
  scope: ExtensionScope;
  scopeName: string;
  skillUpdates: ReturnType<typeof useSkillUpdates>;
  tools: readonly Tool[];
  /** Tools that can hold skills, taken from the page's already-computed `scopedTargets`. */
  copyTargets: readonly Tool[];
  onEdit?: (extension: Extension) => void;
  onRemove?: (extension: Extension) => void;
  onUpdate?: (extension: Extension) => void;
}

export function ExtensionsInventoryPanel({
  activeOperation,
  extensions,
  mutationsBlocked,
  tab,
  task,
  scope,
  scopeName,
  skillUpdates,
  tools,
  copyTargets,
  onEdit,
  onRemove,
  onUpdate,
}: ExtensionsInventoryPanelProps) {
  const { t } = useTranslation();
  // Only the local inventory can answer "where else is this skill installed"
  // — this list here only sees a single tool.
  const inventory = useLocalExtensionInventory();
  const toolNames = useMemo(
    () => new Map(tools.map((tool) => [tool.id, tool.name])),
    [tools],
  );
  const skillOwners = useMemo(
    () => skillOwnersById(inventory.data?.items ?? [], toolNames),
    [inventory.data?.items, toolNames],
  );
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

      {tab.kind === "skill" ? (
        <SkillUpdatesNotice updates={skillUpdates} />
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
            skillOwners={skillOwners}
            copyTargets={copyTargets}
            skillUpdateIds={
              new Set(skillUpdates.data?.map((update) => update.id) ?? [])
            }
            rescanning={extensions.isFetching || inventory.isFetching}
            onEdit={onEdit}
            onRemove={onRemove}
            onRescan={() => {
              void extensions.refetch();
              void inventory.refetch();
            }}
            onUpdate={onUpdate}
          />
        )
      ) : null}
    </>
  );
}
