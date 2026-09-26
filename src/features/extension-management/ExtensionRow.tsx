import { AlertCircle, LoaderCircle } from "lucide-react";
import { useId } from "react";
import { useTranslation } from "react-i18next";
import { extensionScopeKey } from "@/entities/extension";
import { toErrorCopy } from "@/shared/lib/nativeError";
import { Badge } from "@/shared/ui/Badge";
import { ListGroupRow } from "@/shared/ui/ListGroup";
import type { ExtensionToggleFailure } from "./ExtensionCard";
import { ExtensionRowActions } from "./ExtensionRowActions";
import {
  ScopeToggleGroup,
  type ScopeToggleState,
  type ScopeToggleTarget,
} from "./ScopeToggleGroup";
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
  importing?: boolean;
  resourceAction?: "browse" | "edit";
  failure?: ExtensionToggleFailure;
  /** The app whose switch `failure` belongs to. */
  failedKey?: string | null;
  importError?: Error;
  resourceError?: Error;
  updateAvailable?: boolean;
  onToggle: (target: ScopeToggleTarget, enabled: boolean) => void;
  onImport?: () => void;
  onUpdate?: () => void;
  onRemove?: () => void;
  onOpenLocation?: () => void;
  onEditDocument?: () => void;
  onCopy?: () => void;
}

function stateIn(
  row: UnifiedExtensionRow,
  target: ScopeToggleTarget,
): ScopeToggleState {
  const entry = row.entries.get(extensionScopeKey(target.scope));
  if (row.management === "detected") return entry ? "found" : "absent";
  if (!entry) return "absent";
  return entry.enabled ? "on" : "off";
}

/**
 * One Skill or MCP connection: what it is on the left, its few actions, and
 * one switch per app at the right edge, where the columns line up row to row. Like the card it replaces,
 * the row has no field that could show a command, argument or path.
 */
export function ExtensionRow({
  row,
  targets,
  position,
  total,
  busy,
  pendingKey = null,
  pendingLabel,
  importing = false,
  resourceAction,
  failure,
  failedKey = null,
  importError,
  resourceError,
  updateAvailable = false,
  onToggle,
  onImport,
  onUpdate,
  onRemove,
  onOpenLocation,
  onEditDocument,
  onCopy,
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
  const detected = row.management === "detected";
  const shownError = failure?.error ?? importError ?? resourceError;
  const errorCopy = shownError ? toErrorCopy(shownError) : null;
  const errorTitle =
    importError && !failure
      ? t("extensions.adoption.errorTitle", { name: row.name })
      : labels.failureTitle;

  return (
    <ListGroupRow
      role="article"
      aria-labelledby={headingId}
      aria-busy={
        pendingKey !== null || importing || pendingLabel !== undefined
          ? true
          : undefined
      }
      className="flex flex-col gap-2 px-4 py-3"
    >
      <div className="flex min-w-0 flex-wrap items-center gap-x-4 gap-y-2">
        <div className="min-w-0 flex-1 basis-48">
          <div className="flex min-w-0 items-center gap-2">
            <h3
              id={headingId}
              className="truncate text-body font-medium text-content"
            >
              {row.name}
            </h3>
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
        </div>

        <ExtensionRowActions
          detected={detected}
          busy={busy}
          importing={importing}
          resourceAction={resourceAction}
          labels={{
            ...labels,
            importItem: t("extensions.adoption.importNamed", {
              name: row.name,
            }),
          }}
          onImport={onImport}
          onUpdate={updateAvailable ? onUpdate : undefined}
          onRemove={onRemove}
          onOpenLocation={onOpenLocation}
          onEditDocument={onEditDocument}
          onCopy={onCopy}
        />

        <ScopeToggleGroup
          itemName={labels.item}
          targets={targets}
          stateOf={(target) => stateIn(row, target)}
          pendingKey={pendingKey}
          failedKey={failedKey}
          retryLabel={labels.retry}
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
