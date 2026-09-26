import { AlertTriangle, RefreshCw } from "lucide-react";
import { useId, useState } from "react";
import { useTranslation } from "react-i18next";
import { toast } from "sonner";
import {
  type DesktopApp,
  useDesktopApps,
  useLaunchDesktopApp,
  useOpenDesktopAppOfficialDownload,
  useOpenDesktopAppUninstall,
} from "@/entities/desktop-app";
import type { ToolId } from "@/entities/tool";
import { Button } from "@/shared/ui/Button";
import { Card } from "@/shared/ui/Card";
import { cn } from "@/shared/ui/cn";
import { DetectionStatus } from "@/shared/ui/DetectionStatus";
import { DesktopAppCard } from "@/shared/ui/DesktopAppCard";
import { ListGroup } from "@/shared/ui/ListGroup";
import { SectionHeader } from "@/shared/ui/SectionHeader";
import { DesktopAppDownloadHelpModal } from "./DesktopAppDownloadHelpModal";

export interface DesktopAppsSectionProps {
  className?: string;
  onOpenRelatedTool?: (tool: ToolId) => void;
  onManageRelatedTool?: (tool: ToolId) => void;
  onManageAppConfiguration?: (app: DesktopApp) => void;
}

export function DesktopAppsSection({
  className,
  onOpenRelatedTool,
  onManageRelatedTool,
  onManageAppConfiguration,
}: DesktopAppsSectionProps = {}) {
  const { t } = useTranslation();
  const headingId = useId();
  const apps = useDesktopApps();
  const launch = useLaunchDesktopApp();
  const officialDownload = useOpenDesktopAppOfficialDownload();
  const uninstall = useOpenDesktopAppUninstall();
  const [downloadHelpApp, setDownloadHelpApp] = useState<DesktopApp | null>(
    null,
  );

  function resetActions(): void {
    launch.reset();
    officialDownload.reset();
    uninstall.reset();
  }

  function launchApp(app: DesktopApp): void {
    resetActions();
    launch.mutate(app.id, {
      onSuccess: () => {
        toast.success(t("tools.desktopApps.opened", { name: app.name }));
      },
    });
  }

  function openOfficialDownload(app: DesktopApp, onOpened?: () => void): void {
    resetActions();
    officialDownload.mutate(app.id, {
      onSuccess: (outcome) => {
        toast.success(
          t(
            outcome.handoff === "directOfficialPackage"
              ? "tools.desktopApps.officialInstallerOpened"
              : "tools.desktopApps.officialDownloadOpened",
            { name: app.name },
          ),
          // The browser is now on its own, and a vendor address that will not
          // load looks identical to one that is merely slow. Saying what to do
          // here beats burying it in the download-help dialog, which a user
          // watching a blank tab has no reason to open.
          { description: t("tools.desktopApps.officialDownloadNetworkHint") },
        );
        onOpened?.();
      },
    });
  }

  function showDownloadHelp(app: DesktopApp): void {
    resetActions();
    setDownloadHelpApp(app);
  }

  // This only opens the system's own uninstall place; nothing is removed
  // here, so it needs no confirmation.
  function openUninstall(app: DesktopApp): void {
    resetActions();
    uninstall.mutate(app.id, {
      onSuccess: () => {
        toast.success(
          t("tools.desktopApps.uninstallHandoffOpened", { name: app.name }),
          { description: t("tools.desktopApps.uninstall.refresh") },
        );
      },
    });
  }

  const initialLoading = apps.isPending && !apps.data;
  const unavailable = apps.isError && !apps.data;
  const refreshFailed = apps.isRefetchError && Boolean(apps.data);

  return (
    <section
      aria-labelledby={headingId}
      className={cn("flex flex-col gap-3", className)}
    >
      <SectionHeader
        headingId={headingId}
        title={t("tools.desktopApps.title")}
        description={t("tools.desktopApps.description")}
        action={
          !unavailable ? (
            <Button
              variant="secondary"
              size="sm"
              loading={apps.isFetching}
              onClick={() => void apps.refetch()}
            >
              <RefreshCw className="h-4 w-4" aria-hidden="true" />
              {t("tools.desktopApps.refresh")}
            </Button>
          ) : undefined
        }
      />

      {initialLoading ? (
        <DetectionStatus label={t("tools.desktopApps.loading")} />
      ) : null}

      {unavailable ? (
        <Card
          role="alert"
          aria-label={t("tools.desktopApps.error.title")}
          className="flex flex-col gap-4 border-warning/30 bg-warning/5 sm:flex-row sm:items-center sm:justify-between"
        >
          <div className="flex items-start gap-3">
            <AlertTriangle
              className="mt-0.5 h-5 w-5 shrink-0 text-warning"
              aria-hidden="true"
            />
            <div>
              <p className="text-body font-medium text-content">
                {t("tools.desktopApps.error.title")}
              </p>
              <p className="mt-0.5 text-caption leading-5 text-content-muted">
                {t("tools.desktopApps.error.description")}
              </p>
            </div>
          </div>
          <Button
            size="sm"
            className="self-start sm:self-auto"
            loading={apps.isFetching}
            onClick={() => void apps.refetch()}
          >
            {t("tools.desktopApps.refresh")}
          </Button>
        </Card>
      ) : null}

      {refreshFailed ? (
        <aside
          role="alert"
          className="flex items-start gap-3 rounded-lg border border-warning/30 bg-warning/10 px-4 py-3"
        >
          <AlertTriangle
            className="mt-0.5 h-4 w-4 shrink-0 text-warning"
            aria-hidden="true"
          />
          <p className="text-caption leading-5 text-content-muted">
            {t("tools.desktopApps.refreshError")}
          </p>
        </aside>
      ) : null}

      {apps.data ? (
        <ListGroup>
          {apps.data.map((app) => {
            const relatedTool = app.relatedTool;
            return (
              <DesktopAppCard
                key={app.id}
                app={app}
                busy={
                  (launch.isPending && launch.variables === app.id) ||
                  (officialDownload.isPending &&
                    officialDownload.variables === app.id) ||
                  (uninstall.isPending && uninstall.variables === app.id)
                }
                error={
                  launch.isError && launch.variables === app.id
                    ? launch.error
                    : officialDownload.isError &&
                        officialDownload.variables === app.id
                      ? officialDownload.error
                      : uninstall.isError && uninstall.variables === app.id
                        ? uninstall.error
                        : null
                }
                onLaunch={() => launchApp(app)}
                onOpenOfficialDownload={() => openOfficialDownload(app)}
                onOpenUninstall={() => openUninstall(app)}
                onShowDownloadHelp={() => showDownloadHelp(app)}
                onOpenRelatedTool={
                  onOpenRelatedTool && relatedTool
                    ? () => onOpenRelatedTool(relatedTool)
                    : undefined
                }
                onManageRelatedTool={
                  onManageRelatedTool && relatedTool
                    ? () => onManageRelatedTool(relatedTool)
                    : undefined
                }
                onManageAppConfiguration={
                  app.canManageMcp && onManageAppConfiguration
                    ? () => onManageAppConfiguration(app)
                    : undefined
                }
              />
            );
          })}
        </ListGroup>
      ) : null}

      <DesktopAppDownloadHelpModal
        app={downloadHelpApp}
        busy={
          officialDownload.isPending &&
          officialDownload.variables === downloadHelpApp?.id
        }
        error={
          officialDownload.isError &&
          officialDownload.variables === downloadHelpApp?.id
            ? officialDownload.error
            : null
        }
        onClose={() => {
          if (officialDownload.isPending) return;
          officialDownload.reset();
          setDownloadHelpApp(null);
        }}
        onRetry={() => {
          if (!downloadHelpApp) return;
          openOfficialDownload(downloadHelpApp, () => setDownloadHelpApp(null));
        }}
      />
    </section>
  );
}
