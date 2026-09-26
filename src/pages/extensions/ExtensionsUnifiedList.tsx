import { useState } from "react";
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
  SkillCopyModal,
  representativeEntry,
  useAdoptDetected,
  useCopyDetectedSkill,
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
  const copy = useCopyDetectedSkill();
  const [copying, setCopying] = useState<UnifiedExtensionRow | null>(null);
  const busy =
    actionsBlocked ||
    setEnabled.isPending ||
    adopt.isPending ||
    operations.length > 0;
  const toolTargets = targets.flatMap((target) =>
    target.tool === null ? [] : [target.tool],
  );
  const foundIn = (row: UnifiedExtensionRow) =>
    targets.filter((target) => row.entries.has(target.key));

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

  const copySource = copying ? representativeEntry(copying) : null;

  return (
    <TooltipProvider delayDuration={200} skipDelayDuration={100}>
      {copying && copySource ? (
        <SkillCopyModal
          skill={copySource}
          scope={copySource.scope}
          targets={toolTargets
            .filter(
              (tool) =>
                copySource.scope.kind !== "tool" ||
                tool.id !== copySource.scope.id,
            )
            .map((tool) => ({ id: tool.id, name: tool.name }))}
          ownedBy={
            new Set(
              foundIn(copying).flatMap((target) =>
                target.tool === null ? [] : [target.tool.id],
              ),
            )
          }
          mutationsBlocked={actionsBlocked}
          copy={copy}
          onOpenChange={(open) => {
            if (!open) setCopying(null);
          }}
        />
      ) : null}

      <ListGroup>
        {rows.map((row, index) => {
          const managed = row.management === "managed";
          const toggle = setEnabled.variables;
          const toggleTargeted =
            managed &&
            toggle !== undefined &&
            toggle.kind === kind &&
            toggle.extensionId === row.id;
          const toggleFailed = toggleTargeted && setEnabled.isError;
          const adoptTargeted =
            !managed && adopt.variables?.extensionId === row.id;
          const resourceTargeted =
            !managed && openResource.variables?.skillId === row.id;
          const operation = operations.find(
            (candidate) => candidate.extension?.id === row.id,
          );
          const skillFound = !managed && kind === "skill";
          const copyable =
            skillFound &&
            targets.some(
              (target) => target.tool !== null && !row.entries.has(target.key),
            );

          return (
            <ExtensionRow
              key={row.key}
              row={row}
              targets={targets}
              position={index + 1}
              total={rows.length}
              busy={busy}
              pendingKey={
                toggleTargeted && setEnabled.isPending
                  ? extensionScopeKey(toggle.scope)
                  : null
              }
              pendingLabel={
                operation
                  ? operation.messageKey
                    ? t(operation.messageKey)
                    : t("extensions.card.updating")
                  : undefined
              }
              importing={adoptTargeted && adopt.isPending}
              importError={
                adoptTargeted && adopt.isError ? adopt.error : undefined
              }
              failure={
                toggleFailed
                  ? {
                      error: setEnabled.error,
                      intendedEnabled: toggle.enabled,
                    }
                  : undefined
              }
              failedKey={toggleFailed ? extensionScopeKey(toggle.scope) : null}
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
              onToggle={(target, enabled) =>
                // A failed write may have landed part-way, so the retry
                // repeats what the user asked for, not the opposite of what
                // the refreshed switch now shows.
                setEnabled.mutate(
                  toggleFailed && extensionScopeKey(toggle.scope) === target.key
                    ? toggle
                    : {
                        scope: target.scope,
                        kind,
                        extensionId: row.id,
                        enabled,
                      },
                )
              }
              onImport={
                managed
                  ? undefined
                  : () =>
                      adopt.mutate(
                        {
                          kind,
                          extensionId: row.id,
                          scopes: foundIn(row).map((target) => target.scope),
                        },
                        {
                          onSuccess: () =>
                            toast.success(
                              t("extensions.adoption.done", { name: row.name }),
                            ),
                        },
                      )
              }
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
              onCopy={copyable ? () => setCopying(row) : undefined}
            />
          );
        })}
      </ListGroup>
    </TooltipProvider>
  );
}
