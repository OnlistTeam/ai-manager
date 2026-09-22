import * as React from "react";
import * as DropdownMenu from "@radix-ui/react-dropdown-menu";
import {
  AlertCircle,
  ArrowUpRight,
  ArrowUpCircle,
  CheckCircle2,
  CircleDashed,
  Download,
  GitBranch,
  HelpCircle,
  Info,
  Loader2,
  MoreHorizontal,
  RefreshCw,
  Trash2,
  Wrench,
} from "lucide-react";
import { useTranslation } from "react-i18next";
import type { Tool, ToolStatus } from "@/native/schemas/tool";
import { cn } from "./cn";
import { Badge, type BadgeTone } from "./Badge";
import { Button } from "./Button";
import { ListGroupRow } from "./ListGroup";
import { FOCUS_RING } from "./focusRing";
import { ToolArtwork } from "./ToolArtwork";

export type ToolCardAction =
  | "install"
  | "update"
  | "open"
  | "fix"
  | "check"
  | "remove"
  | "version";

/**
 * The card is driven entirely by status plus capabilities. Adding a tool must
 * never require a new branch here (AI_RULES rule 8, spec §11).
 *
 * It renders as a row inside a `ListGroup`: the surrounding card, its border
 * and its rounding belong to the group, so a page of tools reads as one list.
 */
const STATUS_META: Record<
  ToolStatus,
  {
    tone: BadgeTone;
    icon: React.ComponentType<{ className?: string }>;
    labelKey: string;
    primary: ToolCardAction;
    /** Which capability gates the primary action, if any. */
    requires: keyof Tool["capabilities"] | null;
    unavailableLabelKey: string | null;
  }
> = {
  notInstalled: {
    tone: "neutral",
    icon: CircleDashed,
    labelKey: "ds.tool.notInstalled",
    primary: "install",
    requires: "canInstall",
    unavailableLabelKey: "ds.tool.installUnavailable",
  },
  installed: {
    tone: "success",
    icon: CheckCircle2,
    labelKey: "ds.tool.installed",
    primary: "open",
    requires: "canLaunch",
    unavailableLabelKey: "ds.tool.openUnavailable",
  },
  updateAvailable: {
    tone: "warning",
    icon: ArrowUpCircle,
    labelKey: "ds.tool.updateAvailable",
    primary: "update",
    requires: "canUpdate",
    unavailableLabelKey: "ds.tool.updateUnavailable",
  },
  broken: {
    tone: "danger",
    icon: AlertCircle,
    labelKey: "ds.tool.broken",
    primary: "fix",
    requires: "canRepair",
    unavailableLabelKey: "ds.tool.repairUnavailable",
  },
  unknown: {
    tone: "neutral",
    icon: HelpCircle,
    labelKey: "ds.tool.unknown",
    primary: "check",
    requires: null,
    unavailableLabelKey: null,
  },
};

const ACTION_META: Record<
  ToolCardAction,
  {
    labelKey: string;
    ariaLabelKey: string;
    icon: React.ComponentType<{ className?: string }>;
  }
> = {
  install: {
    labelKey: "ds.action.install",
    ariaLabelKey: "ds.action.named.install",
    icon: Download,
  },
  update: {
    labelKey: "ds.action.update",
    ariaLabelKey: "ds.action.named.update",
    icon: ArrowUpCircle,
  },
  open: {
    labelKey: "ds.action.open",
    ariaLabelKey: "ds.action.named.open",
    icon: ArrowUpRight,
  },
  fix: {
    labelKey: "ds.action.fix",
    ariaLabelKey: "ds.action.named.fix",
    icon: Wrench,
  },
  check: {
    labelKey: "ds.action.check",
    ariaLabelKey: "ds.action.named.check",
    icon: RefreshCw,
  },
  remove: {
    labelKey: "ds.action.remove",
    ariaLabelKey: "ds.action.named.remove",
    icon: Trash2,
  },
  version: {
    labelKey: "ds.action.versions",
    ariaLabelKey: "ds.action.named.versions",
    icon: GitBranch,
  },
};

export interface ToolCardProps {
  tool: Tool;
  /** Slot for the tool logo; the card only reserves the space. */
  icon?: React.ReactNode;
  /** Active lifecycle work replaces actions so a disabled card explains itself. */
  activeTask?: React.ReactNode;
  /** Terminal feedback can occupy the task slot without claiming the card is busy. */
  activeTaskBusy?: boolean;
  busy?: boolean;
  /**
   * The latest version for this tool is still being read in the background. It
   * buys one quiet spinner next to the version chip and disables nothing:
   * installing, opening and removing never wait on that answer.
   */
  checkingVersion?: boolean;
  pendingAction?: ToolCardAction | null;
  onAction: (action: ToolCardAction) => void;
  onDetails?: () => void;
  className?: string;
}

export function ToolCard({
  tool,
  icon,
  activeTask,
  activeTaskBusy = true,
  busy = false,
  checkingVersion = false,
  pendingAction = null,
  onAction,
  onDetails,
  className,
}: ToolCardProps) {
  const { t } = useTranslation();
  const meta = STATUS_META[tool.status];
  const primaryCapabilityAvailable =
    meta.requires === null || tool.capabilities[meta.requires];
  const showPrimary = primaryCapabilityAvailable;
  const primaryUnavailable =
    meta.requires !== null && !primaryCapabilityAvailable;
  const showRemove =
    tool.status !== "notInstalled" && tool.capabilities.canUninstall;
  const showOpenAlongsideUpdate =
    tool.status === "updateAvailable" && tool.capabilities.canLaunch;
  const showVersion =
    tool.status !== "notInstalled" && tool.capabilities.canManageVersion;
  const versionText =
    tool.status === "updateAvailable" && tool.version && tool.latestVersion
      ? `${tool.version} → ${tool.latestVersion}`
      : tool.version;
  const verifiedUpToDate =
    tool.status === "installed" &&
    tool.version !== null &&
    tool.latestVersion !== null &&
    tool.version === tool.latestVersion;
  // Mutually exclusive with "up to date": either it is verified or it is still
  // being read. When neither can be said honestly, the card says nothing.
  const checkingLatest =
    checkingVersion && Boolean(versionText) && !verifiedUpToDate;
  const PrimaryIcon = ACTION_META[meta.primary].icon;
  const RemoveIcon = ACTION_META.remove.icon;
  const OpenIcon = ACTION_META.open.icon;
  const VersionIcon = ACTION_META.version.icon;
  const hasActiveTask = activeTask !== null && activeTask !== undefined;

  return (
    <ListGroupRow
      data-tool-card={tool.id}
      aria-busy={busy || (hasActiveTask && activeTaskBusy) || undefined}
      className={cn(
        "group overflow-hidden transition-colors duration-fast ease-standard focus-within:bg-layer-1",
        className,
      )}
    >
      <div className="relative flex min-w-0 flex-col gap-4 lg:flex-row lg:items-center">
        <div className="flex min-w-0 flex-1 items-start gap-3">
          {icon ? (
            <span className="flex h-11 w-11 shrink-0 items-center justify-center rounded-sm border border-hairline bg-layer-1">
              {icon}
            </span>
          ) : (
            <ToolArtwork toolId={tool.id} className="h-11 w-11 rounded-lg" />
          )}
          <div className="min-w-0 flex-1">
            <div className="flex flex-wrap items-center gap-2">
              <h3 className="truncate text-body font-semibold text-content">
                {tool.name}
              </h3>
              <Badge tone={meta.tone} icon={meta.icon} className="shrink-0">
                {t(meta.labelKey)}
              </Badge>
              {versionText ? (
                <span className="inline-flex rounded-sm border border-hairline bg-layer-1 px-2 py-0.5 text-mono-sm font-mono text-content-muted">
                  {versionText}
                </span>
              ) : null}
              {verifiedUpToDate ? (
                <span className="text-caption font-medium text-success">
                  {t("tools.upToDate")}
                </span>
              ) : null}
              {checkingLatest ? (
                <Loader2
                  role="status"
                  aria-label={t("tools.checkingVersion")}
                  className="h-4 w-4 shrink-0 text-content-muted motion-safe:animate-spin"
                />
              ) : null}
            </div>
            <p className="mt-1 text-caption leading-5 text-content-muted">
              {t(tool.descriptionKey)}
            </p>
          </div>
        </div>

        <div
          role="group"
          aria-label={tool.name}
          className={cn(
            "flex min-w-0 flex-wrap items-center gap-2 lg:max-w-[48%] lg:justify-end",
            hasActiveTask && "lg:w-[44%]",
          )}
        >
          {hasActiveTask ? (
            <div className="min-w-0 basis-full">{activeTask}</div>
          ) : (
            <>
              {primaryUnavailable && meta.unavailableLabelKey ? (
                <p className="flex min-w-0 basis-56 grow items-start gap-2 text-caption leading-5 text-content-muted">
                  <Info
                    className="mt-0.5 h-4 w-4 shrink-0"
                    aria-hidden="true"
                  />
                  <span>{t(meta.unavailableLabelKey)}</span>
                </p>
              ) : null}
              {showPrimary ? (
                <Button
                  variant={meta.primary === "open" ? "secondary" : "primary"}
                  data-tool-primary-action=""
                  size="sm"
                  disabled={busy}
                  loading={busy && pendingAction === meta.primary}
                  aria-label={t(ACTION_META[meta.primary].ariaLabelKey, {
                    name: tool.name,
                  })}
                  onClick={() => onAction(meta.primary)}
                >
                  {pendingAction === meta.primary ? null : (
                    <PrimaryIcon className="h-4 w-4" aria-hidden="true" />
                  )}
                  {t(ACTION_META[meta.primary].labelKey)}
                </Button>
              ) : null}
              {showOpenAlongsideUpdate ? (
                <Button
                  variant="secondary"
                  size="sm"
                  disabled={busy}
                  loading={busy && pendingAction === "open"}
                  aria-label={t(ACTION_META.open.ariaLabelKey, {
                    name: tool.name,
                  })}
                  onClick={() => onAction("open")}
                >
                  {pendingAction === "open" ? null : (
                    <OpenIcon className="h-4 w-4" aria-hidden="true" />
                  )}
                  {t(ACTION_META.open.labelKey)}
                </Button>
              ) : null}
            </>
          )}
          {onDetails ? (
            <Button
              variant="secondary"
              size="sm"
              onClick={onDetails}
              aria-label={t("tools.details.title", { name: tool.name })}
            >
              <Info className="h-4 w-4" aria-hidden="true" />
              {t("tools.details.action")}
            </Button>
          ) : null}
          {!hasActiveTask && (showVersion || showRemove) ? (
            <DropdownMenu.Root>
              <DropdownMenu.Trigger asChild>
                <Button
                  variant="ghost"
                  size="sm"
                  disabled={busy}
                  className="w-9 px-0"
                  aria-label={t("tools.actions.moreNamed", {
                    name: tool.name,
                  })}
                >
                  <MoreHorizontal className="h-4 w-4" aria-hidden="true" />
                </Button>
              </DropdownMenu.Trigger>
              <DropdownMenu.Portal>
                <DropdownMenu.Content
                  align="end"
                  sideOffset={8}
                  className="z-[70] min-w-48 rounded-md border border-hairline bg-surface p-1.5 shadow-lg animate-ds-overlay-in"
                >
                  {showVersion ? (
                    <DropdownMenu.Item
                      disabled={busy}
                      onSelect={() => onAction("version")}
                      className={menuItemClass}
                    >
                      <VersionIcon className="h-4 w-4" aria-hidden="true" />
                      {t(ACTION_META.version.labelKey)}
                    </DropdownMenu.Item>
                  ) : null}
                  {showVersion && showRemove ? (
                    <DropdownMenu.Separator className="my-1 h-px bg-hairline" />
                  ) : null}
                  {showRemove ? (
                    <DropdownMenu.Item
                      disabled={busy}
                      onSelect={() => onAction("remove")}
                      className={cn(
                        menuItemClass,
                        "text-danger focus:bg-danger/10 focus:text-danger",
                      )}
                    >
                      <RemoveIcon className="h-4 w-4" aria-hidden="true" />
                      {t(ACTION_META.remove.labelKey)}
                    </DropdownMenu.Item>
                  ) : null}
                </DropdownMenu.Content>
              </DropdownMenu.Portal>
            </DropdownMenu.Root>
          ) : null}
        </div>
      </div>
    </ListGroupRow>
  );
}

const menuItemClass = cn(
  "flex cursor-default select-none items-center gap-2 rounded-md px-3 py-2 text-caption text-content outline-none",
  "data-[disabled]:pointer-events-none data-[disabled]:opacity-50 focus:bg-layer-2",
  FOCUS_RING,
);
