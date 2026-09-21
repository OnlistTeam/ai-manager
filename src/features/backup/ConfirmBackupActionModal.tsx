import { useRef } from "react";
import { AlertCircle, AlertTriangle } from "lucide-react";
import { useTranslation } from "react-i18next";
import type { ErrorCopy } from "@/shared/lib/nativeError";
import { Button } from "@/shared/ui/Button";
import { Modal } from "@/shared/ui/Modal";

export type BackupAction = "restore" | "delete";

/**
 * Copy table for the two destructive actions. Spec §18 requires that we
 * "explain what's about to happen before touching anything the user already
 * has", so every action lists its consequences point by point instead of
 * papering over them with a single "Are you sure?".
 * This is the shape AI_RULES rule 8 allows: a static table pinned down by
 * tests, not scattered branches.
 */
const COPY = {
  restore: {
    titleKey: "preferences.backup.restoreTitle",
    descriptionKey: "preferences.backup.restoreDescription",
    pointKeys: [
      "preferences.backup.restorePoint.replaces",
      "preferences.backup.restorePoint.installed",
      "preferences.backup.restorePoint.followUp",
    ],
    confirmKey: "preferences.backup.restoreConfirm",
  },
  delete: {
    titleKey: "preferences.backup.deleteTitle",
    descriptionKey: "preferences.backup.deleteDescription",
    pointKeys: ["preferences.backup.deletePoint.permanent"],
    confirmKey: "preferences.backup.deleteConfirm",
  },
} as const;

export interface ConfirmBackupActionModalProps {
  /** `null` = closed. While open, also carries the payload for "which one, doing what". */
  action: BackupAction | null;
  /** The human-readable name (date and time) the user recognizes, not the file name. */
  label: string;
  busy?: boolean;
  /** Blocks submission while the authoritative backup list is unsettled. */
  confirmDisabled?: boolean;
  error?: ErrorCopy | null;
  onOpenChange: (open: boolean) => void;
  onConfirm: () => void;
}

export function ConfirmBackupActionModal({
  action,
  label,
  busy = false,
  confirmDisabled = false,
  error = null,
  onOpenChange,
  onConfirm,
}: ConfirmBackupActionModalProps) {
  const { t } = useTranslation();
  const cancelRef = useRef<HTMLButtonElement>(null);
  const copy = action === null ? COPY.restore : COPY[action];
  const restore = action !== "delete";

  return (
    <Modal
      open={action !== null}
      onOpenChange={onOpenChange}
      dismissible={!busy}
      initialFocusRef={cancelRef}
      size="md"
      title={t(copy.titleKey, { name: label })}
      description={t(copy.descriptionKey)}
      footer={
        <>
          <Button
            ref={cancelRef}
            variant="secondary"
            disabled={busy}
            onClick={() => onOpenChange(false)}
          >
            {t("ds.action.cancel")}
          </Button>
          <Button
            variant="danger"
            loading={busy}
            disabled={confirmDisabled}
            onClick={onConfirm}
          >
            {t(copy.confirmKey)}
          </Button>
        </>
      }
    >
      <ul
        className={`flex flex-col gap-2 rounded-md p-3 ${
          restore ? "bg-warning/10" : "bg-danger/10"
        }`}
      >
        {copy.pointKeys.map((key) => (
          <li
            key={key}
            className="flex items-start gap-2 text-caption text-content"
          >
            <AlertTriangle
              className={`mt-0.5 h-4 w-4 shrink-0 ${
                restore ? "text-warning" : "text-danger"
              }`}
              aria-hidden="true"
            />
            {t(key)}
          </li>
        ))}
      </ul>

      {error ? (
        <div
          role="alert"
          className="mt-3 flex items-start gap-2 rounded-md border border-danger/30 bg-danger/5 p-3"
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
              {t("preferences.backup.error.actionRetry")}
            </p>
          </div>
        </div>
      ) : null}
    </Modal>
  );
}
