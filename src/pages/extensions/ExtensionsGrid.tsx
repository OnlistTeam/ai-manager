import { RefreshCw } from "lucide-react";
import { useState } from "react";
import { useTranslation } from "react-i18next";
import { toast } from "sonner";
import type {
  Extension,
  ExtensionKind,
  ExtensionScope,
} from "@/entities/extension";
import { extensionScopeKey } from "@/entities/extension";
import type { Operation } from "@/entities/operation";
import type { SkillOwner } from "./localExtensionGroups";
import type { Tool, ToolId } from "@/entities/tool";
import {
  ExtensionCard,
  SkillCopyModal,
  useCopyDetectedSkill,
  useOpenDetectedSkillResource,
  useSetExtensionEnabled,
} from "@/features/extension-management";
import { ToolOperationProgress } from "@/features/tool-management";
import { Button } from "@/shared/ui/Button";

export interface ExtensionsGridProps {
  scope: ExtensionScope;
  scopeName: string;
  kind: ExtensionKind;
  extensions: Extension[];
  activeOperation?: Operation;
  actionsBlocked: boolean;
  /** Which other tools each local skill also belongs to, indexed by skill id. */
  skillOwners?: ReadonlyMap<string, readonly SkillOwner[]>;
  /** Tools that can hold skills, used as candidate targets for copying. */
  copyTargets?: readonly Tool[];
  skillUpdateIds?: ReadonlySet<string>;
  rescanning?: boolean;
  onEdit?: (extension: Extension) => void;
  onRemove?: (extension: Extension) => void;
  onRescan?: () => void;
  onUpdate?: (extension: Extension) => void;
}

export function ExtensionsGrid({
  scope,
  scopeName,
  kind,
  extensions,
  activeOperation,
  actionsBlocked,
  skillOwners,
  copyTargets,
  skillUpdateIds,
  rescanning = false,
  onEdit,
  onRemove,
  onRescan,
  onUpdate,
}: ExtensionsGridProps) {
  const { t } = useTranslation();
  const setEnabled = useSetExtensionEnabled();
  const openDetectedSkill = useOpenDetectedSkillResource();
  const copyDetectedSkill = useCopyDetectedSkill();
  const [copyingSkill, setCopyingSkill] = useState<Extension | null>(null);
  // Exclude the current tool from the targets: it's where this skill was discovered.
  const copyDestinations = (copyTargets ?? []).filter(
    (tool) => scope.kind !== "tool" || tool.id !== scope.id,
  );
  const failedVariables = setEnabled.isError ? setEnabled.variables : null;
  const failedError = setEnabled.isError ? setEnabled.error : null;
  const busy =
    actionsBlocked || setEnabled.isPending || activeOperation !== undefined;
  const entries = extensions.map((extension, index) => ({ extension, index }));
  const groups =
    kind === "prompt"
      ? []
      : [
          {
            id: "detected",
            entries: entries.filter(
              ({ extension }) => extension.management === "detected",
            ),
          },
          {
            id: "managed",
            entries: entries.filter(
              ({ extension }) => extension.management === "managed",
            ),
          },
        ].filter((group) => group.entries.length > 0);

  function renderCard(extension: Extension, index: number) {
    const operationPending = activeOperation?.extension?.id === extension.id;
    const togglePending =
      setEnabled.isPending &&
      extensionScopeKey(setEnabled.variables.scope) ===
        extensionScopeKey(scope) &&
      setEnabled.variables.kind === kind &&
      setEnabled.variables.extensionId === extension.id;
    const toggleFailed =
      failedVariables !== null &&
      extensionScopeKey(failedVariables.scope) === extensionScopeKey(scope) &&
      failedVariables.kind === kind &&
      failedVariables.extensionId === extension.id &&
      failedError !== null;
    const copyTargeted =
      copyDetectedSkill.variables !== undefined &&
      extensionScopeKey(copyDetectedSkill.variables.scope) ===
        extensionScopeKey(scope) &&
      copyDetectedSkill.variables.skillId === extension.id;
    const detectedResourceTargeted =
      openDetectedSkill.variables !== undefined &&
      extensionScopeKey(openDetectedSkill.variables.scope) ===
        extensionScopeKey(scope) &&
      openDetectedSkill.variables.skillId === extension.id;

    const openDetectedResource = (action: "browse" | "edit") => {
      openDetectedSkill.reset();
      openDetectedSkill.mutate(
        { scope, skillId: extension.id, action },
        {
          onSuccess: () => {
            toast.success(
              t(
                action === "browse"
                  ? "extensions.card.locationOpened"
                  : "extensions.card.documentOpened",
                { name: extension.name },
              ),
            );
          },
        },
      );
    };

    return (
      <ExtensionCard
        key={extension.id}
        extension={extension}
        position={index + 1}
        total={extensions.length}
        presentIn={
          extension.kind === "skill" && extension.management === "detected"
            ? skillOwners?.get(extension.id)?.map((owner) => owner.name)
            : undefined
        }
        busy={busy}
        pending={
          togglePending ||
          operationPending ||
          (copyTargeted && copyDetectedSkill.isPending)
        }
        pendingLabel={
          operationPending && activeOperation.messageKey
            ? t(activeOperation.messageKey)
            : undefined
        }
        failure={
          toggleFailed
            ? {
                error: failedError,
                intendedEnabled: failedVariables.enabled,
              }
            : undefined
        }
        detectedResourceAction={
          detectedResourceTargeted && openDetectedSkill.isPending
            ? openDetectedSkill.variables.action
            : undefined
        }
        detectedResourceError={
          detectedResourceTargeted && openDetectedSkill.isError
            ? openDetectedSkill.error
            : undefined
        }
        updateAvailable={
          extension.kind === "skill" &&
          extension.management === "managed" &&
          skillUpdateIds?.has(extension.id)
        }
        onEdit={
          onEdit && extension.management === "managed"
            ? () => onEdit(extension)
            : undefined
        }
        onRemove={
          onRemove &&
          extension.management === "managed" &&
          (extension.kind !== "prompt" || !extension.enabled)
            ? () => onRemove(extension)
            : undefined
        }
        onUpdate={
          onUpdate &&
          extension.kind === "skill" &&
          extension.management === "managed" &&
          skillUpdateIds?.has(extension.id)
            ? () => onUpdate(extension)
            : undefined
        }
        onOpenLocation={
          extension.kind === "skill" && extension.management === "detected"
            ? () => openDetectedResource("browse")
            : undefined
        }
        onEditDetectedDocument={
          extension.kind === "skill" && extension.management === "detected"
            ? () => openDetectedResource("edit")
            : undefined
        }
        onCopyToTool={
          extension.kind === "skill" &&
          extension.management === "detected" &&
          copyDestinations.some((tool) => !ownedBy(extension.id).has(tool.id))
            ? () => setCopyingSkill(extension)
            : undefined
        }
        onToggle={(enabled) => {
          if (toggleFailed) {
            setEnabled.mutate(failedVariables);
            return;
          }
          setEnabled.mutate({
            scope,
            kind,
            extensionId: extension.id,
            enabled,
          });
        }}
      />
    );
  }

  function ownedBy(skillId: string): ReadonlySet<ToolId> {
    return new Set(
      (skillOwners?.get(skillId) ?? []).map((owner) => owner.tool),
    );
  }

  return (
    <div className="flex flex-col gap-4">
      <SkillCopyModal
        skill={copyingSkill}
        scope={scope}
        targets={copyDestinations.map((tool) => ({
          id: tool.id,
          name: tool.name,
        }))}
        ownedBy={ownedBy(copyingSkill?.id ?? "")}
        mutationsBlocked={actionsBlocked}
        copy={copyDetectedSkill}
        onOpenChange={(open) => {
          if (!open) setCopyingSkill(null);
        }}
      />

      {activeOperation ? (
        <ToolOperationProgress
          operation={activeOperation}
          toolName={scopeName}
        />
      ) : null}

      {kind === "prompt" ? (
        <div className="grid gap-4">
          {entries.map(({ extension, index }) => renderCard(extension, index))}
        </div>
      ) : (
        groups.map((group) => (
          <section
            key={group.id}
            aria-labelledby={`extensions-inventory-${group.id}`}
            className="flex flex-col gap-3"
          >
            <div className="flex min-w-0 items-end justify-between gap-4 border-b border-hairline pb-2">
              <div className="min-w-0">
                <h3
                  id={`extensions-inventory-${group.id}`}
                  className="text-body font-semibold text-content"
                >
                  {t(`extensions.inventory.${group.id}.title`)}
                </h3>
                <p className="mt-0.5 text-caption leading-5 text-content-muted">
                  {t(`extensions.inventory.${group.id}.description`, {
                    tool: scopeName,
                  })}
                </p>
              </div>
              <div className="flex shrink-0 items-center gap-2">
                <span className="rounded-full bg-layer-1 px-2.5 py-1 text-caption tabular-nums text-content-muted">
                  {group.entries.length}
                </span>
                {group.id === "detected" && onRescan ? (
                  <Button
                    size="sm"
                    variant="ghost"
                    loading={rescanning}
                    onClick={onRescan}
                  >
                    <RefreshCw className="h-4 w-4" aria-hidden="true" />
                    {t("extensions.inventory.detected.rescan")}
                  </Button>
                ) : null}
              </div>
            </div>
            <div className="grid gap-4">
              {group.entries.map(({ extension, index }) =>
                renderCard(extension, index),
              )}
            </div>
          </section>
        ))
      )}
    </div>
  );
}
