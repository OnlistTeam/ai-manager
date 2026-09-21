import { useRef } from "react";
import { useTranslation } from "react-i18next";
import type { DeepLinkPreview } from "@/entities/deeplink";
import type { ErrorCopy } from "@/shared/lib/nativeError";
import { Button } from "@/shared/ui/Button";
import { Modal } from "@/shared/ui/Modal";
import { DeepLinkErrorNotice } from "./DeepLinkErrorNotice";
import { DeepLinkSummary } from "./DeepLinkSummary";

export interface DeepLinkConfirmationModalProps {
  open: boolean;
  preview: DeepLinkPreview;
  busy: boolean;
  error: ErrorCopy | null;
  onOpenChange: (open: boolean) => void;
  onConfirm: () => void;
}

/**
 * Nothing is imported until this dialog is accepted (ADR-0029 decision 5). The
 * cancel button holds the initial focus for the same reason it does in the
 * existing-setup import: the safe answer should be the one a stray Enter gives.
 */
export function DeepLinkConfirmationModal({
  open,
  preview,
  busy,
  error,
  onOpenChange,
  onConfirm,
}: DeepLinkConfirmationModalProps) {
  const { t } = useTranslation();
  const cancelRef = useRef<HTMLButtonElement>(null);

  return (
    <Modal
      open={open}
      onOpenChange={onOpenChange}
      dismissible={!busy}
      initialFocusRef={cancelRef}
      title={t("deeplink.confirm.title")}
      description={t("deeplink.confirm.description")}
      footer={
        <>
          <Button
            ref={cancelRef}
            variant="secondary"
            disabled={busy}
            onClick={() => onOpenChange(false)}
          >
            {t("deeplink.confirm.cancel")}
          </Button>
          <Button
            loading={busy}
            disabled={preview.blocked !== null}
            onClick={onConfirm}
          >
            {error ? t("deeplink.confirm.retry") : t("deeplink.confirm.action")}
          </Button>
        </>
      }
    >
      <div className="flex flex-col gap-3">
        <DeepLinkSummary preview={preview} />
        {error ? <DeepLinkErrorNotice error={error} /> : null}
      </div>
    </Modal>
  );
}
