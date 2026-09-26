import { useTranslation } from "react-i18next";
import { toast } from "sonner";
import {
  extensionScopeKey,
  type Extension,
  type ExtensionKind,
} from "@/entities/extension";
import type { Operation } from "@/entities/operation";
import {
  ExtensionRow,
  representativeEntry,
  useAdoptDetected,
  useOpenDetectedSkillResource,
  useSetExtensionEnabled,
  type ExtensionScopeOption,
  type UnifiedExtensionRow,
} from "@/features/extension-management";
import { ListGroup } from "@/shared/ui/ListGroup";
import { TooltipProvider } from "@/shared/ui/Tooltip";

export interface ExtensionsUnifiedListProps {
  kind: ExtensionKind;
  rows: readonly UnifiedExtensionRow[];
  /** Supported apps, in the order their switches appear on every row. */
  targets: readonly ExtensionScopeOption[];
  operations: readonly Operation[];
  actionsBlocked: boolean;
  skillUpdateIds: ReadonlySet<string>;
  onRemove: (extension: Extension) => void;
  onUpdate: (extension: Extension) => void;
}

/** Owns the row-level writes so each row only reports what the user asked for. */
export function ExtensionsUnifiedList({
  kind,
  rows,
  targets,
  operations,
  actionsBlocked,
  skillUpdateIds,
  onRemove,
  onUpdate,
}: ExtensionsUnifiedListProps) {
  const { t } = useTranslation();
  const setEnabled = useSetExtensionEnabled();
  const adopt = useAdoptDetected();
  const openResource = useOpenDetectedSkillResource();
  const busy =
    actionsBlocked ||
    setEnabled.isPending ||
    adopt.isPending ||
    operations.length > 0;
  const foundIn = (row: UnifiedExtensionRow) =>
    targets
      .filter((target) => row.entries.has(target.key))
      .map((target) => target.scope);

  const openDetected = (
    row: UnifiedExtensionRow,
    action: "browse" | "edit",
  ) => {
    const source = representativeEntry(row);
    openResource.reset();
    openResource.mutate(
      { scope: source.scope, skillId: row.id, action },
      {
        onSuccess: () =>
          toast.success(
            t(
              action === "browse"
                ? "extensions.card.locationOpened"
                : "extensions.card.documentOpened",
              { name: row.name },
            ),
          ),
      },
    );
  };

  return (
    <TooltipProvider delayDuration={200} skipDelayDuration={100}>
      <ListGroup>
        {rows.map((row, index) => {
          const managed = row.management === "managed";
          const toggle = setEnabled.variables;
          const toggleTargeted =
            managed &&
            toggle !== undefined &&
            toggle.kind === kind &&
            toggle.extensionId === row.id;
          const adoption = adopt.variables;
          const adoptTargeted =
            !managed &&
            adoption !== undefined &&
            adoption.kind === kind &&
            adoption.extensionId === row.id;
          const toggleFailed =
            (toggleTargeted && setEnabled.isError) ||
            (adoptTargeted && adopt.isError);
          // The switch that was last written for this row, whichever path
          // wrote it.
          const written = toggleTargeted
            ? toggle
            : adoptTargeted
              ? adoption
              : null;
          const writtenKey = written ? extensionScopeKey(written.scope) : null;
          const writeError = toggleTargeted ? setEnabled.error : adopt.error;
          const resourceTargeted =
            !managed && openResource.variables?.skillId === row.id;
          const operation = operations.find(
            (candidate) => candidate.extension?.id === row.id,
          );
          const skillFound = !managed && kind === "skill";

          return (
            <ExtensionRow
              key={row.key}
              row={row}
              targets={targets}
              position={index + 1}
              total={rows.length}
              busy={busy}
              pendingKey={
                (toggleTargeted && setEnabled.isPending) ||
                (adoptTargeted && adopt.isPending)
                  ? writtenKey
                  : null
              }
              pendingLabel={
                operation
                  ? operation.messageKey
                    ? t(operation.messageKey)
                    : t("extensions.card.updating")
                  : undefined
              }
              failure={
                toggleFailed && written && writeError
                  ? { error: writeError, intendedEnabled: written.enabled }
                  : undefined
              }
              failedKey={toggleFailed ? writtenKey : null}
              resourceAction={
                resourceTargeted && openResource.isPending
                  ? openResource.variables.action
                  : undefined
              }
              resourceError={
                resourceTargeted && openResource.isError
                  ? openResource.error
                  : undefined
              }
              updateAvailable={
                managed && kind === "skill" && skillUpdateIds.has(row.id)
              }
              onToggle={(target, enabled) => {
                // A failed write may have landed part-way, so the retry
                // repeats what the user asked for, not the opposite of what
                // the refreshed switch now shows.
                const retry = toggleFailed && writtenKey === target.key;
                if (managed) {
                  setEnabled.mutate(
                    retry && toggleTargeted
                      ? toggle
                      : {
                          scope: target.scope,
                          kind,
                          extensionId: row.id,
                          enabled,
                        },
                  );
                  return;
                }
                // A found item is taken over on its first switch, so it can
                // be turned on or off anywhere like any other.
                adopt.mutate(
                  retry && adoption
                    ? adoption
                    : {
                        kind,
                        extensionId: row.id,
                        found: foundIn(row),
                        scope: target.scope,
                        enabled,
                      },
                );
              }}
              onUpdate={() => onUpdate(representativeEntry(row))}
              onRemove={
                managed ? () => onRemove(representativeEntry(row)) : undefined
              }
              onOpenLocation={
                skillFound ? () => openDetected(row, "browse") : undefined
              }
              onEditDocument={
                skillFound ? () => openDetected(row, "edit") : undefined
              }
            />
          );
        })}
      </ListGroup>
    </TooltipProvider>
  );
}
