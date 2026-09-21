import {
  ArrowRight,
  Check,
  CheckCircle2,
  Circle,
  Copy,
  FilePenLine,
  FolderOpen,
  HardDrive,
  LoaderCircle,
  Pencil,
  RefreshCw,
  Trash2,
} from "lucide-react";
import { useTranslation } from "react-i18next";
import type { Extension } from "@/entities/extension";
import { Badge } from "@/shared/ui/Badge";
import { Button } from "@/shared/ui/Button";
import { Switch } from "@/shared/ui/Switch";
import type { ExtensionCardLabels } from "./useExtensionCardLabels";

export interface ExtensionCardFooterProps {
  extension: Extension;
  /** The extension was found on disk rather than installed by the product. */
  detected: boolean;
  busy: boolean;
  pending: boolean;
  pendingLabel?: string;
  /** The last toggle failed, so the primary control offers a retry. */
  failed: boolean;
  detectedResourceAction?: "browse" | "edit";
  updateAvailable: boolean;
  labels: ExtensionCardLabels;
  onOpenLocation?: () => void;
  onEditDetectedDocument?: () => void;
  onCopyToTool?: () => void;
  onEdit?: () => void;
  onRemove?: () => void;
  onUpdate?: () => void;
  onToggle: (enabled: boolean) => void;
}

/** The card's bottom row: a status line on the left, every action on the right. */
export function ExtensionCardFooter({
  extension,
  detected,
  busy,
  pending,
  pendingLabel,
  failed,
  detectedResourceAction,
  updateAvailable,
  labels,
  onOpenLocation,
  onEditDetectedDocument,
  onCopyToTool,
  onEdit,
  onRemove,
  onUpdate,
  onToggle,
}: ExtensionCardFooterProps) {
  const { t } = useTranslation();

  return (
    <div className="relative mt-auto flex flex-wrap items-center justify-between gap-3 border-t border-hairline bg-layer-1 px-5 py-3.5">
      {detected ? (
        <p className="flex min-w-0 items-start gap-2 text-caption leading-5 text-content-muted">
          <HardDrive
            className="mt-0.5 h-4 w-4 shrink-0 text-brand"
            aria-hidden="true"
          />
          <span>{labels.presence}</span>
        </p>
      ) : pending ? (
        <p
          role="status"
          className="flex items-center gap-2 text-caption text-content-muted"
        >
          <LoaderCircle
            className="h-4 w-4 text-brand motion-safe:animate-spin"
            aria-hidden="true"
          />
          {pendingLabel ?? t("extensions.card.updating")}
        </p>
      ) : (
        <p className="flex items-center gap-2 text-caption text-content-muted">
          {extension.enabled ? (
            <CheckCircle2 className="h-4 w-4 text-success" aria-hidden="true" />
          ) : (
            <Circle className="h-4 w-4 text-content-muted" aria-hidden="true" />
          )}
          {extension.enabled
            ? t("extensions.card.on")
            : t("extensions.card.off")}
        </p>
      )}

      <div className="flex flex-wrap items-center justify-end gap-2">
        {detected && extension.kind === "skill" && onOpenLocation ? (
          <Button
            size="sm"
            variant="ghost"
            aria-label={labels.openLocation}
            disabled={busy || detectedResourceAction !== undefined}
            loading={detectedResourceAction === "browse"}
            onClick={onOpenLocation}
          >
            <FolderOpen className="h-4 w-4" aria-hidden="true" />
            {t("extensions.card.openLocation")}
          </Button>
        ) : null}

        {detected && extension.kind === "skill" && onCopyToTool ? (
          <Button
            size="sm"
            variant="ghost"
            aria-label={labels.copyTo}
            disabled={busy || pending}
            loading={pending}
            onClick={onCopyToTool}
          >
            <Copy className="h-4 w-4" aria-hidden="true" />
            {t("extensions.card.copyTo")}
          </Button>
        ) : null}

        {detected && extension.kind === "skill" && onEditDetectedDocument ? (
          <Button
            size="sm"
            variant="secondary"
            aria-label={labels.editDetected}
            disabled={busy || detectedResourceAction !== undefined}
            loading={detectedResourceAction === "edit"}
            onClick={onEditDetectedDocument}
          >
            <FilePenLine className="h-4 w-4" aria-hidden="true" />
            {t("extensions.card.editDocument")}
          </Button>
        ) : null}

        {!detected && updateAvailable && onUpdate ? (
          <Button
            size="sm"
            variant="secondary"
            aria-label={labels.update}
            disabled={busy}
            onClick={onUpdate}
          >
            <RefreshCw className="h-4 w-4" aria-hidden="true" />
            {t("extensions.card.update")}
          </Button>
        ) : null}

        {!detected && onEdit ? (
          <Button
            size="sm"
            variant="ghost"
            aria-label={labels.edit}
            disabled={busy}
            onClick={onEdit}
          >
            <Pencil className="h-4 w-4" aria-hidden="true" />
            {t("extensions.card.edit")}
          </Button>
        ) : null}

        {!detected && onRemove ? (
          <Button
            size="sm"
            variant="ghost"
            className="text-danger hover:bg-danger/10 hover:text-danger"
            aria-label={labels.remove}
            disabled={busy}
            onClick={onRemove}
          >
            <Trash2 className="h-4 w-4" aria-hidden="true" />
            {t("extensions.card.remove")}
          </Button>
        ) : null}

        {detected ? null : extension.canDisable ? (
          <Switch
            aria-label={labels.retry ?? labels.item}
            checked={extension.enabled}
            disabled={busy}
            onCheckedChange={onToggle}
          />
        ) : extension.enabled && !failed ? (
          <Badge tone="success" icon={Check}>
            {t("ds.service.nowActive")}
          </Badge>
        ) : (
          <Button
            variant="secondary"
            aria-label={labels.retry ?? labels.use}
            disabled={busy}
            loading={pending}
            onClick={() => onToggle(true)}
          >
            {failed ? (
              <RefreshCw className="h-4 w-4" aria-hidden="true" />
            ) : (
              <ArrowRight className="h-4 w-4" aria-hidden="true" />
            )}
            {failed ? t("extensions.card.retry") : t("ds.action.use")}
          </Button>
        )}
      </div>
    </div>
  );
}
