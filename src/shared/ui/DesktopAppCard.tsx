import {
  AlertCircle,
  AlertTriangle,
  Ban,
  CheckCircle2,
  CircleDashed,
  RefreshCw,
} from "lucide-react";
import { useId } from "react";
import { useTranslation } from "react-i18next";
import type { DesktopApp, DesktopAppStatus } from "@/native/schemas/desktopApp";
import { toErrorCopy } from "@/shared/lib/nativeError";
import { Badge, type BadgeTone } from "./Badge";
import { ListGroupRow } from "./ListGroup";
import { DesktopAppActions } from "./DesktopAppActions";
import { DesktopAppArtwork } from "./DesktopAppArtwork";

const STATUS_META: Record<
  DesktopAppStatus,
  {
    tone: BadgeTone;
    icon: typeof CheckCircle2;
  }
> = {
  installed: { tone: "success", icon: CheckCircle2 },
  updateAvailable: { tone: "warning", icon: RefreshCw },
  notInstalled: { tone: "neutral", icon: CircleDashed },
  unsupported: { tone: "neutral", icon: Ban },
  unknown: { tone: "warning", icon: AlertTriangle },
};

interface DesktopAppCardProps {
  app: DesktopApp;
  busy: boolean;
  error: Error | null;
  onLaunch: () => void;
  onOpenOfficialDownload: () => void;
  onOpenUninstall: () => void;
  onShowDownloadHelp: () => void;
  onOpenRelatedTool?: () => void;
  onManageRelatedTool?: () => void;
  onManageAppConfiguration?: () => void;
}

export function DesktopAppCard({
  app,
  busy,
  error,
  onLaunch,
  onOpenOfficialDownload,
  onOpenUninstall,
  onShowDownloadHelp,
  onOpenRelatedTool,
  onManageRelatedTool,
  onManageAppConfiguration,
}: DesktopAppCardProps) {
  const { t } = useTranslation();
  const titleId = useId();
  const status = STATUS_META[app.status];
  const errorCopy = error ? toErrorCopy(error) : null;
  const relatedTool = app.relatedTool
    ? t(`tools.desktopApps.relatedTools.${app.relatedTool}`)
    : null;

  return (
    <ListGroupRow
      data-desktop-app-card={app.id}
      role="article"
      aria-labelledby={titleId}
      className="overflow-hidden p-0 transition-colors duration-fast ease-standard focus-within:bg-layer-1"
    >
      <div className="p-4">
        <div className="flex min-w-0 flex-col gap-4 lg:flex-row lg:items-center">
          <div className="flex min-w-0 flex-1 items-start gap-3">
            <DesktopAppArtwork appId={app.id} className="bg-layer-1" />
            <div className="min-w-0 flex-1">
              <div className="flex flex-wrap items-center gap-2">
                <h3
                  id={titleId}
                  className="text-body font-semibold text-content"
                >
                  {app.name}
                </h3>
                <Badge
                  tone={status.tone}
                  icon={status.icon}
                  data-status={app.status}
                >
                  {t(`tools.desktopApps.status.${app.status}`)}
                </Badge>
                {app.version ? (
                  <span className="inline-flex rounded-md border border-hairline bg-layer-1 px-2 py-0.5 text-mono-sm font-mono text-content-muted">
                    {app.version}
                  </span>
                ) : null}
              </div>
              <p className="mt-1 text-caption leading-5 text-content-muted">
                {t(`tools.desktopApps.items.${app.id}.description`)}
              </p>
              <div className="mt-1.5 flex flex-wrap gap-x-4 gap-y-1 text-caption text-content-muted">
                <span>
                  {app.status === "updateAvailable" && app.latestVersion
                    ? t("tools.desktopApps.details.officialUpdateAvailable", {
                        version: app.latestVersion,
                      })
                    : app.updatesManagedByVendor
                      ? t("tools.desktopApps.details.vendorManagedUpdates")
                      : t("tools.desktopApps.details.updateUnavailable")}
                </span>
                <span>
                  {t(
                    `tools.desktopApps.relationship.${app.configurationRelationship}`,
                    relatedTool ? { tool: relatedTool } : undefined,
                  )}
                </span>
              </div>
            </div>
          </div>
          <DesktopAppActions
            app={app}
            busy={busy}
            relatedToolName={relatedTool}
            onLaunch={onLaunch}
            onOpenOfficialDownload={onOpenOfficialDownload}
            onOpenUninstall={onOpenUninstall}
            onShowDownloadHelp={onShowDownloadHelp}
            onOpenRelatedTool={onOpenRelatedTool}
            onManageRelatedTool={onManageRelatedTool}
            onManageAppConfiguration={onManageAppConfiguration}
          />
        </div>

        {errorCopy ? (
          <div
            role="alert"
            className="mt-3 flex items-start gap-2 rounded-md border border-danger/30 bg-danger/5 p-3"
          >
            <AlertCircle
              className="mt-0.5 h-4 w-4 shrink-0 text-danger"
              aria-hidden="true"
            />
            <div className="min-w-0 text-caption leading-5 text-content-muted">
              <p className="font-medium text-content">
                {t(errorCopy.messageKey)}
              </p>
              {errorCopy.remediationKey ? (
                <p>{t(errorCopy.remediationKey)}</p>
              ) : null}
            </div>
          </div>
        ) : null}
      </div>
    </ListGroupRow>
  );
}
