import { AlertCircle, Download, Globe2, ShieldCheck } from "lucide-react";
import { useTranslation } from "react-i18next";
import type { DesktopApp } from "@/entities/desktop-app";
import { toErrorCopy } from "@/shared/lib/nativeError";
import { Button } from "@/shared/ui/Button";
import { Modal } from "@/shared/ui/Modal";

interface DesktopAppDownloadHelpModalProps {
  app: DesktopApp | null;
  busy: boolean;
  error: Error | null;
  onClose: () => void;
  onRetry: () => void;
}

export function DesktopAppDownloadHelpModal({
  app,
  busy,
  error,
  onClose,
  onRetry,
}: DesktopAppDownloadHelpModalProps) {
  const { t } = useTranslation();
  const errorCopy = error ? toErrorCopy(error) : null;

  if (!app) return null;

  return (
    <Modal
      open
      onOpenChange={(open) => {
        if (!open && !busy) onClose();
      }}
      dismissible={!busy}
      title={t("tools.desktopApps.downloadHelp.title", { name: app.name })}
      description={t(
        app.installerHandoff === "directOfficialPackage"
          ? "tools.desktopApps.downloadHelp.directDescription"
          : "tools.desktopApps.downloadHelp.pageDescription",
      )}
      size="sm"
      footer={
        <>
          <Button variant="ghost" disabled={busy} onClick={onClose}>
            {t("ds.action.cancel")}
          </Button>
          <Button loading={busy} onClick={onRetry}>
            <Download className="h-4 w-4" aria-hidden="true" />
            {t("tools.desktopApps.downloadHelp.retry")}
          </Button>
        </>
      }
    >
      <div className="grid gap-3">
        <HelpItem icon={Download}>
          {t("tools.desktopApps.downloadHelp.architecture")}
        </HelpItem>
        <HelpItem icon={ShieldCheck}>
          {t("tools.desktopApps.downloadHelp.systemOwned")}
        </HelpItem>
        <HelpItem icon={Globe2}>
          {t("tools.desktopApps.downloadHelp.restrictedNetwork")}
        </HelpItem>
        {errorCopy ? (
          <div
            role="alert"
            className="flex items-start gap-2 rounded-lg border border-danger/30 bg-danger/5 p-3 text-caption leading-5 text-content-muted"
          >
            <AlertCircle
              className="mt-0.5 h-4 w-4 shrink-0 text-danger"
              aria-hidden="true"
            />
            <div>
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
    </Modal>
  );
}

function HelpItem({
  icon: Icon,
  children,
}: {
  icon: typeof Download;
  children: string;
}) {
  return (
    <div className="flex items-start gap-3 rounded-lg border border-hairline bg-layer-1 p-3 text-caption leading-5 text-content-muted">
      <Icon className="mt-0.5 h-4 w-4 shrink-0 text-brand" aria-hidden="true" />
      <p>{children}</p>
    </div>
  );
}
