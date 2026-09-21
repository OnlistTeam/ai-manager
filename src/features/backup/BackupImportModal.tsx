import { AlertCircle, KeyRound, ShieldAlert, Upload } from "lucide-react";
import { useRef } from "react";
import { useTranslation } from "react-i18next";
import type { ErrorCopy } from "@/shared/lib/nativeError";
import { Button } from "@/shared/ui/Button";
import { Modal } from "@/shared/ui/Modal";

export interface BackupImportModalProps {
  open: boolean;
  blocked: boolean;
  importing: boolean;
  error: ErrorCopy | null;
  onOpenChange: (open: boolean) => void;
  onImport: () => void;
}

const CONFIRMATIONS = [
  "preferences.backup.transfer.confirmReplace",
  "preferences.backup.transfer.confirmSafety",
  "preferences.backup.transfer.confirmTrust",
  "preferences.backup.transfer.confirmInstalled",
] as const;

/** Import replaces the managed database, so it always reads its consequences first. */
export function BackupImportModal({
  open,
  blocked,
  importing,
  error,
  onOpenChange,
  onImport,
}: BackupImportModalProps) {
  const { t } = useTranslation();
  const cancelRef = useRef<HTMLButtonElement>(null);

  return (
    <Modal
      open={open}
      size="md"
      dismissible={!importing}
      initialFocusRef={cancelRef}
      onOpenChange={onOpenChange}
      title={t("preferences.backup.transfer.confirmTitle")}
      description={t("preferences.backup.transfer.confirmDescription")}
      footer={
        <>
          <Button
            ref={cancelRef}
            variant="secondary"
            disabled={importing}
            onClick={() => onOpenChange(false)}
          >
            {t("ds.action.cancel")}
          </Button>
          <Button
            variant="danger"
            loading={importing}
            disabled={blocked}
            onClick={onImport}
          >
            <Upload className="h-4 w-4" aria-hidden="true" />
            {t("preferences.backup.transfer.chooseAndImport")}
          </Button>
        </>
      }
    >
      <div className="flex flex-col gap-4">
        <div className="flex items-start gap-3 rounded-xl border border-warning/30 bg-warning/10 p-4">
          <KeyRound
            className="mt-0.5 h-5 w-5 shrink-0 text-warning"
            aria-hidden="true"
          />
          <div className="min-w-0">
            <p className="font-medium text-content">
              {t("preferences.backup.transfer.secretTitle")}
            </p>
            <p className="mt-1 text-caption leading-5 text-content-muted">
              {t("preferences.backup.transfer.secretDescription")}
            </p>
          </div>
        </div>

        <ul className="flex flex-col gap-2 rounded-xl bg-danger/5 p-4">
          {CONFIRMATIONS.map((key) => (
            <li
              key={key}
              className="flex items-start gap-2 text-caption leading-5 text-content"
            >
              <ShieldAlert
                className="mt-0.5 h-4 w-4 shrink-0 text-danger"
                aria-hidden="true"
              />
              {t(key)}
            </li>
          ))}
        </ul>

        {error ? (
          <div
            role="alert"
            aria-label={t(error.messageKey)}
            className="flex items-start gap-2 rounded-xl border border-danger/30 bg-danger/5 p-3"
          >
            <AlertCircle
              className="mt-0.5 h-4 w-4 shrink-0 text-danger"
              aria-hidden="true"
            />
            <div>
              <p className="text-caption font-medium text-content">
                {t(error.messageKey)}
              </p>
              <p className="mt-0.5 text-caption leading-5 text-content-muted">
                {t("preferences.backup.transfer.errorHint")}
              </p>
            </div>
          </div>
        ) : null}
      </div>
    </Modal>
  );
}
