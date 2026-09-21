import { useTranslation } from "react-i18next";
import type { ErrorCopy } from "@/shared/lib/nativeError";
import { Modal } from "@/shared/ui/Modal";

export interface ErrorDetailsModalProps {
  /** `null` = closed. */
  error: ErrorCopy | null;
  /** Human-readable operation name; never a raw command or filesystem path. */
  taskName: string | null;
  onOpenChange: (open: boolean) => void;
}

/**
 * Spec §42's View Details: the **only** place allowed to display
 * `technicalMessage`. The backend has already redacted and truncated it
 * (`platform::redact`); this component's only job is to avoid treating it
 * as the primary copy, and to keep it from blowing out the modal's layout.
 */
export function ErrorDetailsModal({
  error,
  taskName,
  onOpenChange,
}: ErrorDetailsModalProps) {
  const { t } = useTranslation();

  return (
    <Modal
      open={error !== null}
      onOpenChange={onOpenChange}
      title={t("taskCenter.details.title")}
      description={taskName ?? undefined}
      size="lg"
    >
      {error ? (
        <div className="flex flex-col gap-4">
          <p className="text-body text-content">{t(error.messageKey)}</p>
          {error.remediationKey ? (
            <p className="text-body text-content-muted">
              {t(error.remediationKey)}
            </p>
          ) : null}

          <dl className="grid grid-cols-[auto_1fr] gap-x-4 gap-y-1 text-caption">
            <dt className="text-content-muted">
              {t("taskCenter.details.code")}
            </dt>
            <dd className="font-mono text-mono-sm text-content">
              {error.code}
            </dd>
            {error.contextId ? (
              <>
                <dt className="text-content-muted">
                  {t("taskCenter.details.reference")}
                </dt>
                <dd className="font-mono text-mono-sm text-content">
                  {error.contextId}
                </dd>
              </>
            ) : null}
          </dl>

          <div className="flex flex-col gap-1">
            <span className="text-caption text-content-muted">
              {t("taskCenter.details.technical")}
            </span>
            {error.technicalMessage ? (
              <pre className="max-h-48 overflow-auto whitespace-pre-wrap rounded-md bg-layer-1 p-3 font-mono text-mono-sm text-content">
                {error.technicalMessage}
              </pre>
            ) : (
              <p className="text-caption text-content-muted">
                {t("taskCenter.details.noTechnical")}
              </p>
            )}
          </div>
        </div>
      ) : null}
    </Modal>
  );
}
