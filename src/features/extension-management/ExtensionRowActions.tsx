import {
  FilePenLine,
  FolderOpen,
  Pencil,
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
  busy: boolean;
  resourceAction?: "browse" | "edit";
  labels: ExtensionCardLabels;
  onOpenLocation?: () => void;
  onEditDocument?: () => void;
  onEdit?: () => void;
  onUpdate?: () => void;
  onRemove?: () => void;
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
 * The row's few actions, each offered only when the list passes a handler for
 * it: a Skill's folder and SKILL.md, an MCP connection's edit form, Update
 * when one is known, and Remove for what AI Manager keeps. Rarely used actions
 * are icons with a tooltip so the switches stay the row's focus.
 */
export function ExtensionRowActions({
  busy,
  resourceAction,
  labels,
  onOpenLocation,
  onEditDocument,
  onEdit,
  onUpdate,
  onRemove,
}: ExtensionRowActionsProps) {
  const { t } = useTranslation();

  return (
    <div className="flex shrink-0 items-center gap-1">
      {onOpenLocation ? (
        <IconAction
          icon={FolderOpen}
          label={labels.openLocation}
          tooltip={t("extensions.card.openLocation")}
          disabled={busy || resourceAction !== undefined}
          loading={resourceAction === "browse"}
          onClick={onOpenLocation}
        />
      ) : null}
      {onEditDocument ? (
        <IconAction
          icon={FilePenLine}
          label={labels.editDetected}
          tooltip={t("extensions.card.editDocument")}
          disabled={busy || resourceAction !== undefined}
          loading={resourceAction === "edit"}
          onClick={onEditDocument}
        />
      ) : null}
      {onEdit ? (
        <IconAction
          icon={Pencil}
          label={labels.edit}
          tooltip={t("extensions.card.edit")}
          disabled={busy}
          onClick={onEdit}
        />
      ) : null}
      {onUpdate ? (
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
      {onRemove ? (
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
