import { Download, RotateCw } from "lucide-react";
import { useTranslation } from "react-i18next";
import { toast } from "sonner";
import {
  useInstallAppUpdate,
  useUpdateStatus,
  type UpdateStatus,
} from "@/entities/update";
import { toErrorCopy } from "@/shared/lib/nativeError";
import { Button } from "@/shared/ui/Button";

/**
 * A deliberately quiet global update affordance. The product updater remains
 * invisible while idle and only enters the persistent chrome when there is a
 * useful state the user can act on or monitor.
 */
export function GlobalUpdateAction() {
  const { t } = useTranslation();
  const status = useUpdateStatus();
  const install = useInstallAppUpdate();
  const phase = status.data?.phase;

  if (phase === "downloading") {
    const progress = downloadPercent(status.data);

    return (
      <span
        role="status"
        aria-label={t("preferences.updates.backgroundDownloading")}
        className="app-update-status inline-flex h-9 items-center gap-2 rounded-full border border-brand/20 bg-layer-1 px-3.5 text-caption text-content-muted shadow-[inset_0_1px_0_hsl(var(--content)/0.06),0_10px_24px_hsl(var(--shadow-color)/0.08)]"
      >
        <Download
          className="h-3.5 w-3.5 animate-pulse text-brand"
          aria-hidden="true"
        />
        <span>{t("preferences.updates.backgroundDownloading")}</span>
        {progress === null ? null : (
          <span className="font-medium tabular-nums text-content">
            {progress}%
          </span>
        )}
      </span>
    );
  }

  if (phase !== "ready") return null;

  return (
    <Button
      size="sm"
      variant="secondary"
      loading={install.isPending}
      className="app-update-ready rounded-full border-success/25 bg-success/10 px-4 text-content shadow-[inset_0_1px_0_hsl(var(--content)/0.08),0_10px_26px_hsl(var(--success)/0.08)] hover:border-success/40 hover:bg-success/15"
      onClick={() => {
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
      }}
    >
      <RotateCw className="h-3.5 w-3.5 text-success" aria-hidden="true" />
      {t("preferences.updates.restart")}
    </Button>
  );
}

function downloadPercent(status: UpdateStatus | undefined): number | null {
  if (!status?.totalBytes || status.totalBytes <= 0) return null;
  return Math.min(
    100,
    Math.round((status.downloadedBytes / status.totalBytes) * 100),
  );
}
