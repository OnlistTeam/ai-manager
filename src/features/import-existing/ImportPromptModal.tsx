import { useEffect, useRef, useState } from "react";
import { useTranslation } from "react-i18next";
import { useImportPreview, useRunImport } from "@/entities/import";
import { toErrorCopy } from "@/shared/lib/nativeError";
import { Button } from "@/shared/ui/Button";
import { Modal } from "@/shared/ui/Modal";
import { ImportCompletion } from "./ImportCompletion";
import { ImportDecisionNotice } from "./ImportDecisionNotice";
import { ImportRefreshNotice } from "./ImportDiscoveryNotice";
import { ImportErrorNotice } from "./ImportErrorNotice";
import { ImportSafetyNotice } from "./ImportSafetyNotice";
import { ImportSourceNotice } from "./ImportSourceNotice";
import { ImportSummary } from "./ImportSummary";

export interface ImportPromptModalProps {
  enabled: boolean;
  onResolve: () => Promise<void>;
}

type DecisionAction = "skip" | "continue";

/** §17 startup decision. While visible, the only exits are the literal Import and Skip choices. */
export function ImportPromptModal({
  enabled,
  onResolve,
}: ImportPromptModalProps) {
  const { t } = useTranslation();
  const preview = useImportPreview(enabled);
  const run = useRunImport();
  const [resolved, setResolved] = useState(false);
  const [decisionAction, setDecisionAction] = useState<DecisionAction | null>(
    null,
  );
  const [decisionPending, setDecisionPending] = useState(false);
  const [decisionFailed, setDecisionFailed] = useState(false);
  const continueRef = useRef<HTMLButtonElement>(null);
  const skipRef = useRef<HTMLButtonElement>(null);
  const importRef = useRef<HTMLButtonElement>(null);
  const discoveryRetryRef = useRef<HTMLButtonElement>(null);
  const previewData = preview.data;
  const sourceAvailable = previewData?.available ?? false;
  const error = run.isError ? toErrorCopy(run.error) : null;
  const imported = run.data?.imported ?? null;
  const discoveryRefreshFailed = preview.isError && sourceAvailable;
  const discoveryBlocked = preview.isFetching || preview.isError;
  const choicesBlocked = run.isPending || decisionPending;
  const open =
    !resolved &&
    (enabled || decisionPending || decisionFailed) &&
    (sourceAvailable || imported !== null || decisionPending || decisionFailed);

  useEffect(() => {
    if (imported) continueRef.current?.focus({ preventScroll: true });
  }, [imported]);

  useEffect(() => {
    if (!decisionFailed || decisionPending) return;

    const target = imported ? continueRef.current : skipRef.current;
    const active = document.activeElement;
    const dialog = target?.closest<HTMLElement>("[role='dialog']");
    if (
      target &&
      (active === document.body || active === dialog || active === target)
    ) {
      target.focus({ preventScroll: true });
    }
  }, [decisionFailed, decisionPending, imported]);

  const retryDiscovery = () => {
    const retryButton = discoveryRetryRef.current;
    const dialog = retryButton?.closest<HTMLElement>("[role='dialog']");
    const restoreFocus = document.activeElement === retryButton;

    void preview.refetch().then((result) => {
      window.requestAnimationFrame(() => {
        const active = document.activeElement;
        const focusStayedWithRetry =
          active === document.body ||
          active === retryButton ||
          active === dialog ||
          (active instanceof HTMLElement && !active.isConnected);
        if (!restoreFocus || !focusStayedWithRetry) return;

        if (result.isSuccess && result.data.available) {
          importRef.current?.focus({ preventScroll: true });
        } else if (result.isError) {
          discoveryRetryRef.current?.focus({ preventScroll: true });
        }
      });
    });
  };

  const resolve = async (action: DecisionAction) => {
    if (decisionPending) return;

    setDecisionAction(action);
    setDecisionPending(true);
    setDecisionFailed(false);
    try {
      await onResolve();
      setResolved(true);
    } catch {
      setDecisionFailed(true);
    } finally {
      setDecisionPending(false);
    }
  };

  return (
    <Modal
      open={open}
      onOpenChange={() => undefined}
      dismissible={false}
      title={t(
        imported
          ? "preferences.import.completion.title"
          : "preferences.import.prompt.title",
      )}
      description={t(
        imported
          ? "preferences.import.completion.description"
          : "preferences.import.prompt.description",
      )}
      footer={
        imported ? (
          <Button
            ref={continueRef}
            loading={decisionPending && decisionAction === "continue"}
            disabled={decisionPending}
            onClick={() => void resolve("continue")}
          >
            {t("preferences.import.completion.continue")}
          </Button>
        ) : (
          <>
            <Button
              ref={skipRef}
              variant="secondary"
              loading={decisionPending && decisionAction === "skip"}
              disabled={choicesBlocked}
              onClick={() => void resolve("skip")}
            >
              {t("preferences.import.skip")}
            </Button>
            <Button
              ref={importRef}
              loading={run.isPending}
              disabled={decisionPending || discoveryBlocked}
              onClick={() => {
                if (!discoveryBlocked && !decisionPending) run.mutate();
              }}
            >
              {error
                ? t("preferences.import.retryAction")
                : t("preferences.import.action")}
            </Button>
          </>
        )
      }
    >
      {imported ? (
        <div className="flex flex-col gap-3">
          <ImportCompletion summary={imported} showHeading={false} />
          {decisionFailed ? <ImportDecisionNotice /> : null}
        </div>
      ) : (
        <div className="flex flex-col gap-3">
          {discoveryRefreshFailed ? (
            <ImportRefreshNotice
              refreshing={preview.isFetching}
              disabled={choicesBlocked}
              retryButtonRef={discoveryRetryRef}
              onRetry={retryDiscovery}
            />
          ) : null}
          <ImportSourceNotice />
          {previewData ? <ImportSummary summary={previewData.summary} /> : null}
          <ImportSafetyNotice />
          {error ? <ImportErrorNotice error={error} /> : null}
          {decisionFailed ? <ImportDecisionNotice /> : null}
        </div>
      )}
    </Modal>
  );
}
