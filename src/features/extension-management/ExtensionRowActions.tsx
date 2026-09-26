import {
  FilePenLine,
  FolderOpen,
  RefreshCw,
  Trash2,
  type LucideIcon,
} from "lucide-react";
import { useTranslation } from "react-i18next";
import { Button } from "@/shared/ui/Button";
import { cn } from "@/shared/ui/cn";
import { Tooltip, TooltipContent, TooltipTrigger } from "@/shared/ui/Tooltip";
import type { ExtensionCardLabels } from "./useExtensionCardLabels";

export interface ExtensionRowActionsProps {
  detected: boolean;
  busy: boolean;
  resourceAction?: "browse" | "edit";
  labels: ExtensionCardLabels;
  onUpdate?: () => void;
  onRemove?: () => void;
  onOpenLocation?: () => void;
  onEditDocument?: () => void;
}

function IconAction({
  icon: Icon,
  label,
  tooltip,
  danger = false,
  disabled,
  loading = false,
  onClick,
}: {
  icon: LucideIcon;
  label: string;
  tooltip: string;
  danger?: boolean;
  disabled: boolean;
  loading?: boolean;
  onClick: () => void;
}) {
  return (
    <Tooltip>
      <TooltipTrigger asChild>
        <Button
          size="xs"
          variant="ghost"
          aria-label={label}
          disabled={disabled}
          loading={loading}
          onClick={onClick}
          className={cn(
            "w-7 px-0",
            danger && "hover:bg-danger/10 hover:text-danger",
          )}
        >
          <Icon className="h-4 w-4" aria-hidden="true" />
        </Button>
      </TooltipTrigger>
      <TooltipContent side="bottom">{tooltip}</TooltipContent>
    </Tooltip>
  );
}

/**
 * A found item offers the read-only file actions it always had; a managed one
 * offers Update when one is known and Remove. Rarely used actions are icons
 * with a tooltip so the switches stay the row's focus.
 */
export function ExtensionRowActions({
  detected,
  busy,
  resourceAction,
  labels,
  onUpdate,
  onRemove,
  onOpenLocation,
  onEditDocument,
}: ExtensionRowActionsProps) {
  const { t } = useTranslation();

  return (
    <div className="flex shrink-0 items-center gap-1">
      {detected && onOpenLocation ? (
        <IconAction
          icon={FolderOpen}
          label={labels.openLocation}
          tooltip={t("extensions.card.openLocation")}
          disabled={busy || resourceAction !== undefined}
          loading={resourceAction === "browse"}
          onClick={onOpenLocation}
        />
      ) : null}
      {detected && onEditDocument ? (
        <IconAction
          icon={FilePenLine}
          label={labels.editDetected}
          tooltip={t("extensions.card.editDocument")}
          disabled={busy || resourceAction !== undefined}
          loading={resourceAction === "edit"}
          onClick={onEditDocument}
        />
      ) : null}
      {!detected && onUpdate ? (
        <Button
          size="xs"
          variant="secondary"
          aria-label={labels.update}
          disabled={busy}
          onClick={onUpdate}
        >
          <RefreshCw className="h-4 w-4" aria-hidden="true" />
          {t("extensions.card.update")}
        </Button>
      ) : null}
      {!detected && onRemove ? (
        <IconAction
          icon={Trash2}
          label={labels.remove}
          tooltip={t("extensions.card.remove")}
          danger
          disabled={busy}
          onClick={onRemove}
        />
      ) : null}
    </div>
  );
}
