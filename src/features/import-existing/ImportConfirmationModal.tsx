import { useRef } from "react";
import { useTranslation } from "react-i18next";
import type { ImportSummary as ImportSummaryValue } from "@/entities/import";
import type { ErrorCopy } from "@/shared/lib/nativeError";
import { Button } from "@/shared/ui/Button";
import { Modal } from "@/shared/ui/Modal";
import { ImportRefreshNotice } from "./ImportDiscoveryNotice";
import { ImportErrorNotice } from "./ImportErrorNotice";
import { ImportSafetyNotice } from "./ImportSafetyNotice";
import { ImportSourceNotice } from "./ImportSourceNotice";
import { ImportSummary } from "./ImportSummary";

export interface ImportConfirmationModalProps {
  open: boolean;
  summary: ImportSummaryValue;
  busy: boolean;
  discoveryBlocked: boolean;
  discoveryRefreshFailed: boolean;
  discoveryRefreshing: boolean;
  error: ErrorCopy | null;
  onOpenChange: (open: boolean) => void;
  onRetryDiscovery: () => Promise<{ isSuccess: boolean }>;
  onDiscoveryFallbackFocus: () => void;
  onConfirm: () => void;
}

/** Settings allows repeat import, so it gets an explicit review step before matching rows change. */
export function ImportConfirmationModal({
  open,
  summary,
  busy,
  discoveryBlocked,
  discoveryRefreshFailed,
  discoveryRefreshing,
  error,
  onOpenChange,
  onRetryDiscovery,
  onDiscoveryFallbackFocus,
  onConfirm,
}: ImportConfirmationModalProps) {
  const { t } = useTranslation();
  const cancelRef = useRef<HTMLButtonElement>(null);
  const confirmRef = useRef<HTMLButtonElement>(null);
  const discoveryRetryRef = useRef<HTMLButtonElement>(null);

  function retryDiscovery() {
    const retryButton = discoveryRetryRef.current;
    const dialog = retryButton?.closest<HTMLElement>("[role='dialog']");
    const restoreFocus = document.activeElement === retryButton;
    void onRetryDiscovery().then((result) => {
      window.requestAnimationFrame(() => {
        const active = document.activeElement;
        if (
          !restoreFocus ||
          (active !== document.body &&
            active !== retryButton &&
            active !== dialog)
        ) {
          return;
        }
        if (result.isSuccess) {
          if (confirmRef.current?.isConnected) {
            confirmRef.current.focus({ preventScroll: true });
          } else {
            onDiscoveryFallbackFocus();
          }
        } else {
          discoveryRetryRef.current?.focus({ preventScroll: true });
        }
      });
    });
  }

  return (
    <Modal
      open={open}
      onOpenChange={onOpenChange}
      dismissible={!busy}
      initialFocusRef={cancelRef}
      title={t("preferences.import.confirm.title")}
      description={t("preferences.import.confirm.description")}
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
            ref={confirmRef}
            loading={busy}
            disabled={discoveryBlocked}
            onClick={onConfirm}
          >
            {error
              ? t("preferences.import.retryAction")
              : t("preferences.import.action")}
          </Button>
        </>
      }
    >
      <div className="flex flex-col gap-3">
        {discoveryRefreshFailed ? (
          <ImportRefreshNotice
            refreshing={discoveryRefreshing}
            retryButtonRef={discoveryRetryRef}
            onRetry={retryDiscovery}
          />
        ) : null}
        <ImportSourceNotice />
        <ImportSummary summary={summary} />
        <ImportSafetyNotice />
        {error ? <ImportErrorNotice error={error} /> : null}
      </div>
    </Modal>
  );
}
