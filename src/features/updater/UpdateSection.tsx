import { ExternalLink, RefreshCw, RotateCw } from "lucide-react";
import { useTranslation } from "react-i18next";
import { toast } from "sonner";
import {
  useCheckForUpdate,
  useInstallAppUpdate,
  useOpenAppDownloadPage,
  useUpdateStatus,
  type UpdateStatus,
} from "@/entities/update";
import { toErrorCopy } from "@/shared/lib/nativeError";
import { Badge } from "@/shared/ui/Badge";
import { Button } from "@/shared/ui/Button";
import { Card } from "@/shared/ui/Card";
import { Progress } from "@/shared/ui/Progress";
import { SectionHeader } from "@/shared/ui/SectionHeader";
import { SettingRow } from "@/shared/ui/SettingRow";
import { UPDATE_BADGES, updateTone } from "./updateBadge";

/**
 * Spec §61. When the product's update endpoint, public key, and N→N+1
 * evidence aren't ready yet, the update boundary fails closed: it neither
 * reaches the network nor installs anything. A build in this state is a
 * dev build, and the UI explains it with neutral copy — "dev builds don't
 * check for updates" — instead of a warning badge or a button that can
 * never be pressed.
 */
export function UpdateSection() {
  const { t } = useTranslation();
  const status = useUpdateStatus();
  const check = useCheckForUpdate();
  const install = useInstallAppUpdate();
  const openDownloadPage = useOpenAppDownloadPage();
  const available = status.data?.availableVersion ?? null;
  const channelReady = status.data?.channelReady ?? false;
  const phase = status.data?.phase;
  const devBuild = phase === "unconfigured";
  const failed = status.isError || check.isError || phase === "failed";
  const checking =
    !status.isError &&
    (status.data === undefined || phase === "idle" || phase === "checking");
  const downloading = phase === "downloading";
  const ready = phase === "ready";
  const retrying = (checking || downloading) && (status.data?.attempt ?? 0) > 1;
  const meta =
    UPDATE_BADGES[
      updateTone({
        checking,
        failed,
        phase,
        channelReady: status.data?.channelReady,
        availableVersion: available,
      })
    ];
  const label = failed
    ? t("preferences.updates.error.title")
    : ready && available
      ? t("preferences.updates.readyTitle", { version: available })
      : downloading && available
        ? t("preferences.updates.downloadingTitle", { version: available })
        : status.data
          ? t("preferences.updates.currentVersion", {
              version: status.data.currentVersion,
            })
          : t("preferences.updates.checkingTitle");
  const description = failed
    ? t("preferences.updates.error.description")
    : retrying && status.data
      ? t("preferences.updates.retryingNote", {
          attempt: status.data.attempt,
          max: status.data.maxAttempts,
        })
      : checking
        ? t("preferences.updates.checkingNote")
        : downloading
          ? t("preferences.updates.downloadingNote")
          : ready
            ? t("preferences.updates.readyNote")
            : devBuild
              ? t("preferences.updates.devBuildNote")
              : available
                ? t("preferences.updates.availableVersion", {
                    version: available,
                  })
                : t("preferences.updates.note");

  return (
    <section className="flex flex-col gap-4">
      <SectionHeader
        as="h3"
        title={t("preferences.updates.title")}
        description={t("preferences.updates.description")}
        action={
          devBuild ? undefined : (
            <Button
              variant={ready ? "primary" : "secondary"}
              loading={install.isPending || check.isPending || checking}
              disabled={downloading || (!failed && !channelReady)}
              onClick={() => {
                if (ready) {
                  install.mutate(undefined, {
                    onError: (error) => {
                      const copy = toErrorCopy(error);
                      toast.error(t(copy.messageKey), {
                        description: copy.remediationKey
                          ? t(copy.remediationKey)
                          : undefined,
                      });
                    },
                  });
                  return;
                }
                if (status.isError) {
                  void status.refetch();
                } else {
                  check.mutate();
                }
              }}
            >
              {ready ? (
                <RotateCw className="h-4 w-4" aria-hidden="true" />
              ) : !checking ? (
                <RefreshCw className="h-4 w-4" aria-hidden="true" />
              ) : null}
              {t(
                ready
                  ? "preferences.updates.restart"
                  : downloading
                    ? "preferences.updates.backgroundDownloading"
                    : failed
                      ? "preferences.updates.retry"
                      : "preferences.updates.check",
              )}
            </Button>
          )
        }
      />
      <Card
        role={failed ? "alert" : undefined}
        aria-label={failed ? label : undefined}
        aria-busy={checking || downloading || undefined}
        className={failed ? "border-warning/30 bg-warning/5" : undefined}
      >
        <SettingRow label={label} description={description}>
          <Badge tone={meta.tone} icon={meta.icon}>
            {t(meta.labelKey)}
          </Badge>
        </SettingRow>
        {downloading ? (
          <Progress
            className="mt-4"
            value={downloadProgress(status.data)}
            indeterminate={!status.data?.totalBytes}
            label={t("preferences.updates.downloadProgress")}
          />
        ) : null}
        {failed ? (
          <div className="mt-4 flex justify-end">
            <Button
              variant="secondary"
              loading={openDownloadPage.isPending}
              onClick={() => {
                openDownloadPage.mutate(undefined, {
                  onError: (error) => {
                    const copy = toErrorCopy(error);
                    toast.error(t(copy.messageKey), {
                      description: copy.remediationKey
                        ? t(copy.remediationKey)
                        : undefined,
                    });
                  },
                });
              }}
            >
              <ExternalLink className="h-4 w-4" aria-hidden="true" />
              {t("preferences.updates.openDownloadPage")}
            </Button>
          </div>
        ) : null}
      </Card>
    </section>
  );
}

function downloadProgress(status: UpdateStatus | undefined) {
  if (!status?.totalBytes || status.totalBytes <= 0) return 0;
  return (status.downloadedBytes / status.totalBytes) * 100;
}
