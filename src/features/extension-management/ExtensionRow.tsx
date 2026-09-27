import { AlertCircle, LoaderCircle } from "lucide-react";
import { useId } from "react";
import { useTranslation } from "react-i18next";
import { extensionScopeKey } from "@/entities/extension";
import { toErrorCopy } from "@/shared/lib/nativeError";
import { Badge } from "@/shared/ui/Badge";
import { ListGroupRow } from "@/shared/ui/ListGroup";
import type { ExtensionToggleFailure } from "./ExtensionCard";
import { ExtensionRowActions } from "./ExtensionRowActions";
import { PortabilityHint, usePortabilityCaution } from "./PortabilityHint";
import { ScopeToggleGroup, type ScopeToggleTarget } from "./ScopeToggleGroup";
import {
  representativeEntry,
  type UnifiedExtensionRow,
} from "./unifiedExtensionRows";
import { useExtensionCardLabels } from "./useExtensionCardLabels";

export interface ExtensionRowProps {
  row: UnifiedExtensionRow;
  targets: readonly ScopeToggleTarget[];
  /** Position in the list, so same-named rows keep distinct control names. */
  position: number;
  total: number;
  busy: boolean;
  /** The app whose switch is being written for this row. */
  pendingKey?: string | null;
  /** A background task for this item, e.g. removal. */
  pendingLabel?: string;
  resourceAction?: "browse" | "edit";
  failure?: ExtensionToggleFailure;
  /** The app whose switch `failure` belongs to. */
  failedKey?: string | null;
  resourceError?: Error;
  updateAvailable?: boolean;
  onToggle: (target: ScopeToggleTarget, enabled: boolean) => void;
  onUpdate?: () => void;
  onRemove?: () => void;
  onEdit?: () => void;
  onOpenLocation?: () => void;
  onEditDocument?: () => void;
}

/** A found item is on wherever it was found; the switch takes it over. */
function isOnIn(row: UnifiedExtensionRow, target: ScopeToggleTarget): boolean {
  return row.entries.get(extensionScopeKey(target.scope))?.enabled ?? false;
}

/**
 * One Skill or MCP connection: what it is on the left (name, description and
 * one detail line saying what it runs or where it lives), its few actions,
 * and one switch per app at the right edge, where the columns line up row to
 * row. The detail is display text from native with secrets already masked;
 * it never shows environment or header values (ADR-0062).
 */
export function ExtensionRow({
  row,
  targets,
  position,
  total,
  busy,
  pendingKey = null,
  pendingLabel,
  resourceAction,
  failure,
  failedKey = null,
  resourceError,
  updateAvailable = false,
  onToggle,
  onUpdate,
  onRemove,
  onEdit,
  onOpenLocation,
  onEditDocument,
}: ExtensionRowProps) {
  const { t } = useTranslation();
  const headingId = useId();
  const labels = useExtensionCardLabels(
    representativeEntry(row),
    position,
    total,
    failure,
    resourceError,
    undefined,
  );
  const cautionFor = usePortabilityCaution(row);
  const shownError = failure?.error ?? resourceError;
  const errorCopy = shownError ? toErrorCopy(shownError) : null;
  const errorTitle = labels.failureTitle;

  return (
    <ListGroupRow
      role="article"
      aria-labelledby={headingId}
      aria-busy={
        pendingKey !== null || pendingLabel !== undefined ? true : undefined
      }
      className="flex flex-col gap-2 px-4 py-3"
    >
      <div className="flex min-w-0 flex-wrap items-center gap-x-4 gap-y-2">
        <div className="min-w-0 flex-1 basis-48">
          <div className="flex min-w-0 items-center gap-1.5">
            <h3
              id={headingId}
              className="truncate text-body font-medium text-content"
            >
              {row.name}
            </h3>
            <PortabilityHint row={row} targets={targets} />
            {updateAvailable ? (
              <Badge tone="warning">
                {t("extensions.card.updateAvailable")}
              </Badge>
            ) : null}
          </div>
          {pendingLabel ? (
            <p
              role="status"
              className="mt-0.5 flex items-center gap-1.5 text-caption text-content-muted"
            >
              <LoaderCircle
                className="h-3.5 w-3.5 text-brand motion-safe:animate-spin"
                aria-hidden="true"
              />
              {pendingLabel}
            </p>
          ) : row.description ? (
            <p
              className="mt-0.5 truncate text-caption text-content-muted"
              title={row.description}
            >
              {row.description}
            </p>
          ) : null}
          {row.detail ? (
            <p
              className="mt-0.5 truncate font-mono text-mono-sm text-content-muted"
              title={row.detail}
            >
              {row.detail}
            </p>
          ) : null}
        </div>

        <ExtensionRowActions
          busy={busy}
          resourceAction={resourceAction}
          labels={labels}
          onOpenLocation={onOpenLocation}
          onEditDocument={onEditDocument}
          onEdit={onEdit}
          onUpdate={updateAvailable ? onUpdate : undefined}
          onRemove={onRemove}
        />

        <ScopeToggleGroup
          itemName={labels.item}
          targets={targets}
          isOn={(target) => isOnIn(row, target)}
          pendingKey={pendingKey}
          failedKey={failedKey}
          retryLabel={labels.retry}
          cautionFor={cautionFor}
          disabled={busy}
          onToggle={onToggle}
        />
      </div>

      {errorCopy && errorTitle ? (
        <div
          role="alert"
          aria-label={errorTitle}
          className="flex items-start gap-2 rounded-md border border-danger/30 bg-danger/5 p-3"
        >
          <AlertCircle
            className="mt-0.5 h-4 w-4 shrink-0 text-danger"
            aria-hidden="true"
          />
          <div className="min-w-0 text-caption leading-5">
            <p className="font-medium text-content">{errorTitle}</p>
            <p className="text-content-muted">{t(errorCopy.messageKey)}</p>
            {errorCopy.remediationKey ? (
              <p className="text-content-muted">
                {t(errorCopy.remediationKey)}
              </p>
            ) : null}
            {failure ? (
              <p className="text-content-muted">
                {t("extensions.card.retryHint")}
              </p>
            ) : null}
          </div>
        </div>
      ) : null}
    </ListGroupRow>
  );
}
