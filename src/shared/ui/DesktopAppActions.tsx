import * as DropdownMenu from "@radix-ui/react-dropdown-menu";
import {
  ChevronDown,
  Download,
  ExternalLink,
  HelpCircle,
  RefreshCw,
  Settings2,
  Terminal,
  Trash2,
} from "lucide-react";
import { useTranslation } from "react-i18next";
import { useRef } from "react";
import type { DesktopApp } from "@/native/schemas/desktopApp";
import { Button } from "./Button";
import { cn } from "./cn";
import { FOCUS_RING } from "./focusRing";

interface DesktopAppActionsProps {
  app: DesktopApp;
  busy: boolean;
  onLaunch: () => void;
  onOpenOfficialDownload: () => void;
  onOpenUninstall: () => void;
  onShowDownloadHelp: () => void;
  relatedToolName: string | null;
  onOpenRelatedTool?: () => void;
  onManageRelatedTool?: () => void;
  onManageAppConfiguration?: () => void;
}

export function DesktopAppActions({
  app,
  busy,
  onLaunch,
  onOpenOfficialDownload,
  onOpenUninstall,
  onShowDownloadHelp,
  relatedToolName,
  onOpenRelatedTool,
  onManageRelatedTool,
  onManageAppConfiguration,
}: DesktopAppActionsProps) {
  const { t } = useTranslation();
  const focusMovedByNavigation = useRef(false);
  const installed =
    app.status === "installed" || app.status === "updateAvailable";
  const canDownload = app.installerHandoff !== "unsupported";
  const canUninstall = installed && app.uninstallHandoff !== "unsupported";
  const hasDirectInstaller = app.installerHandoff === "directOfficialPackage";
  const canOpenRelatedTool = Boolean(relatedToolName && onOpenRelatedTool);
  const canManageRelatedTool = Boolean(relatedToolName && onManageRelatedTool);
  const hasRelatedActions = Boolean(
    canOpenRelatedTool || canManageRelatedTool || onManageAppConfiguration,
  );
  const canShowMenu = canDownload || canUninstall || hasRelatedActions;
  const primaryDownloads =
    (app.status === "notInstalled" ||
      app.status === "unsupported" ||
      app.status === "updateAvailable") &&
    canDownload;
  const primaryLaunches = app.canLaunch && app.status !== "updateAvailable";
  const unavailableAction =
    app.status === "unsupported"
      ? t("tools.desktopApps.action.unsupported")
      : t("tools.desktopApps.action.unavailable");
  const unavailableActionAriaLabel =
    app.status === "unsupported"
      ? t("tools.desktopApps.action.unsupportedNamed", { name: app.name })
      : t("tools.desktopApps.action.unavailableNamed", { name: app.name });
  const primaryDownloadLabel =
    app.status === "updateAvailable"
      ? "tools.desktopApps.action.downloadUpdate"
      : hasDirectInstaller
        ? "tools.desktopApps.action.downloadInstaller"
        : "tools.desktopApps.action.installOfficially";
  // Only a fixed official package is a download. Everything else opens the
  // vendor's page in a browser, and the icon has to say which one happens.
  const PrimaryDownloadIcon = hasDirectInstaller ? Download : ExternalLink;
  const primaryDownloadAriaLabel = t(
    app.status === "updateAvailable"
      ? "tools.desktopApps.action.downloadUpdateNamed"
      : "tools.desktopApps.action.installOfficiallyNamed",
    { name: app.name },
  );

  const primary = primaryLaunches ? (
    <Button
      loading={busy}
      className={cn(canShowMenu && "rounded-r-none")}
      aria-label={t("tools.desktopApps.action.openNamed", { name: app.name })}
      onClick={onLaunch}
    >
      <ExternalLink className="h-4 w-4" aria-hidden="true" />
      {t("tools.desktopApps.action.open")}
    </Button>
  ) : primaryDownloads ? (
    <Button
      loading={busy}
      className={cn(canShowMenu && "rounded-r-none")}
      aria-label={primaryDownloadAriaLabel}
      onClick={onOpenOfficialDownload}
    >
      <PrimaryDownloadIcon className="h-4 w-4" aria-hidden="true" />
      {t(primaryDownloadLabel)}
    </Button>
  ) : (
    <Button
      variant="secondary"
      disabled
      className={cn(canShowMenu && "rounded-r-none")}
      aria-label={unavailableActionAriaLabel}
    >
      {app.status === "unknown" ? (
        <RefreshCw className="h-4 w-4" aria-hidden="true" />
      ) : null}
      {unavailableAction}
    </Button>
  );

  return (
    <div className="flex items-center justify-end">
      {primary}
      {canShowMenu ? (
        <DropdownMenu.Root>
          <DropdownMenu.Trigger asChild>
            <Button
              aria-label={t("tools.desktopApps.action.moreNamed", {
                name: app.name,
              })}
              disabled={busy}
              className="-ml-px w-10 rounded-l-none px-0"
            >
              <ChevronDown className="h-4 w-4" aria-hidden="true" />
            </Button>
          </DropdownMenu.Trigger>
          <DropdownMenu.Portal>
            <DropdownMenu.Content
              align="end"
              sideOffset={8}
              onCloseAutoFocus={(event) => {
                if (!focusMovedByNavigation.current) return;
                focusMovedByNavigation.current = false;
                event.preventDefault();
                onOpenRelatedTool?.();
              }}
              className="app-floating-menu z-[70] min-w-56 rounded-lg border p-1.5 animate-ds-overlay-in"
            >
              {app.status === "updateAvailable" && app.canLaunch ? (
                <DropdownMenu.Item
                  disabled={busy}
                  onSelect={onLaunch}
                  className={menuItemClass}
                >
                  <ExternalLink className="h-4 w-4" aria-hidden="true" />
                  {t("tools.desktopApps.action.open")}
                </DropdownMenu.Item>
              ) : null}
              {app.status === "installed" && canDownload ? (
                <DropdownMenu.Item
                  disabled={busy}
                  onSelect={onOpenOfficialDownload}
                  className={menuItemClass}
                >
                  <PrimaryDownloadIcon className="h-4 w-4" aria-hidden="true" />
                  {t(
                    hasDirectInstaller
                      ? "tools.desktopApps.action.downloadLatest"
                      : "tools.desktopApps.action.installOfficially",
                  )}
                </DropdownMenu.Item>
              ) : null}
              {canOpenRelatedTool ? (
                <DropdownMenu.Item
                  disabled={busy}
                  onSelect={() => {
                    focusMovedByNavigation.current = true;
                  }}
                  className={menuItemClass}
                >
                  <Terminal className="h-4 w-4" aria-hidden="true" />
                  {t("tools.desktopApps.action.viewRelatedTool", {
                    tool: relatedToolName,
                  })}
                </DropdownMenu.Item>
              ) : null}
              {canManageRelatedTool ? (
                <DropdownMenu.Item
                  disabled={busy}
                  onSelect={onManageRelatedTool}
                  className={menuItemClass}
                >
                  <Settings2 className="h-4 w-4" aria-hidden="true" />
                  {t(
                    `tools.desktopApps.action.${app.configurationRelationship}.manageTool`,
                    { tool: relatedToolName },
                  )}
                </DropdownMenu.Item>
              ) : null}
              {app.canManageMcp && onManageAppConfiguration ? (
                <DropdownMenu.Item
                  disabled={busy}
                  onSelect={onManageAppConfiguration}
                  className={menuItemClass}
                >
                  <Settings2 className="h-4 w-4" aria-hidden="true" />
                  {t("tools.desktopApps.action.manageAppMcp", {
                    app: app.name,
                  })}
                </DropdownMenu.Item>
              ) : null}
              {canUninstall ? (
                <DropdownMenu.Item
                  disabled={busy}
                  onSelect={onOpenUninstall}
                  className={cn(
                    menuItemClass,
                    "text-danger focus:bg-danger/10 focus:text-danger",
                  )}
                >
                  <Trash2 className="h-4 w-4" aria-hidden="true" />
                  {t("tools.desktopApps.action.uninstall")}
                </DropdownMenu.Item>
              ) : null}
              {canDownload && (installed || hasRelatedActions) ? (
                <DropdownMenu.Separator className="my-1 h-px bg-line" />
              ) : null}
              {canDownload ? (
                <DropdownMenu.Item
                  disabled={busy}
                  onSelect={onShowDownloadHelp}
                  className={menuItemClass}
                >
                  <HelpCircle className="h-4 w-4" aria-hidden="true" />
                  {t("tools.desktopApps.action.downloadHelp")}
                </DropdownMenu.Item>
              ) : null}
            </DropdownMenu.Content>
          </DropdownMenu.Portal>
        </DropdownMenu.Root>
      ) : null}
    </div>
  );
}

const menuItemClass = cn(
  "flex cursor-default select-none items-center gap-2 rounded-md px-3 py-2 text-caption text-content outline-none",
  "data-[disabled]:pointer-events-none data-[disabled]:opacity-50 focus:bg-layer-2",
  FOCUS_RING,
);
