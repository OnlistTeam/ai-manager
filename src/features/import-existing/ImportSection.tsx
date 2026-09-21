import { useId, useState } from "react";
import { CheckCircle2, Download, RefreshCw } from "lucide-react";
import { useTranslation } from "react-i18next";
import { useRunImport } from "@/entities/import";
import { toErrorCopy } from "@/shared/lib/nativeError";
import { Button } from "@/shared/ui/Button";
import { Card } from "@/shared/ui/Card";
import { ImportConfirmationModal } from "./ImportConfirmationModal";
import {
  ImportDiscoveryUnavailable,
  ImportRefreshNotice,
} from "./ImportDiscoveryNotice";
import { ImportErrorNotice } from "./ImportErrorNotice";
import { useImportDiscoveryRecovery } from "./useImportDiscoveryRecovery";

/**
 * One row at the end of Backup and restore: "re-import the existing setup on
 * this computer" plus one button. Counts, source and safety notes live in the
 * confirmation dialog, where the decision is actually made.
 */
export function ImportSection() {
  const { t } = useTranslation();
  const {
    preview,
    sectionRef,
    retryButtonRef,
    discoveryAvailable,
    discoveryInitiallyLoading,
    discoveryUnavailable,
    discoveryRefreshFailed,
    discoveryActionsBlocked,
    retryDiscovery,
    focusDiscoveryResult,
  } = useImportDiscoveryRecovery();
  const run = useRunImport();
  const headingId = useId();
  const [confirmOpen, setConfirmOpen] = useState(false);
  const importPreview = preview.data;
  const canImport = importPreview?.available ?? false;
  const notFound = discoveryAvailable && importPreview?.available === false;
  const actionsBlocked = run.isPending || discoveryActionsBlocked;
  const error = run.isError ? toErrorCopy(run.error) : null;
  const status = discoveryInitiallyLoading
    ? t("preferences.import.loading")
    : notFound
      ? t("preferences.import.notFound.title")
      : run.data
        ? t("preferences.import.completion.title")
        : null;
  const hint = discoveryInitiallyLoading
    ? t("preferences.import.loading")
    : notFound
      ? t("preferences.import.notFound.description")
      : run.data
        ? t("preferences.import.completion.description")
        : t("preferences.import.description");

  return (
    <section
      ref={sectionRef}
      aria-label={t("preferences.import.title")}
      tabIndex={-1}
      className="flex flex-col gap-3 outline-none"
    >
      {discoveryUnavailable ? (
        <ImportDiscoveryUnavailable
          refreshing={preview.isFetching}
          retryButtonRef={retryButtonRef}
          onRetry={retryDiscovery}
        />
      ) : null}

      {discoveryRefreshFailed && !confirmOpen ? (
        <ImportRefreshNotice
          refreshing={preview.isFetching}
          retryButtonRef={retryButtonRef}
          onRetry={retryDiscovery}
        />
      ) : null}

      {discoveryInitiallyLoading || discoveryAvailable ? (
        <Card
          padding="none"
          role={status ? "status" : undefined}
          aria-label={status ?? undefined}
          aria-busy={discoveryInitiallyLoading || undefined}
          className="rounded-xl"
        >
          <div className="flex flex-wrap items-center justify-between gap-3 p-4">
            <div className="flex min-w-0 items-start gap-3">
              <span className="flex h-9 w-9 shrink-0 items-center justify-center rounded-lg bg-layer-1 text-brand">
                {run.data ? (
                  <CheckCircle2
                    className="h-4 w-4 text-success"
                    aria-hidden="true"
                  />
                ) : (
                  <Download className="h-4 w-4" aria-hidden="true" />
                )}
              </span>
              <div className="min-w-0">
                <h3
                  id={headingId}
                  className="text-body font-medium text-content"
                >
                  {t("preferences.import.reimport")}
                </h3>
                <p className="mt-0.5 text-caption leading-5 text-content-muted">
                  {hint}
                </p>
              </div>
            </div>
            {notFound && !discoveryRefreshFailed ? (
              <Button
                ref={retryButtonRef}
                variant="secondary"
                loading={preview.isFetching}
                onClick={retryDiscovery}
              >
                <RefreshCw className="h-4 w-4" aria-hidden="true" />
                {t("preferences.import.retry")}
              </Button>
            ) : canImport ? (
              <Button
                variant="secondary"
                disabled={actionsBlocked}
                onClick={() => {
                  if (!actionsBlocked) setConfirmOpen(true);
                }}
              >
                {run.isSuccess
                  ? t("preferences.import.again")
                  : t("preferences.import.action")}
              </Button>
            ) : null}
          </div>
        </Card>
      ) : null}

      {error && !confirmOpen ? <ImportErrorNotice error={error} /> : null}

      {canImport && importPreview ? (
        <ImportConfirmationModal
          open={confirmOpen}
          summary={importPreview.summary}
          busy={run.isPending}
          discoveryBlocked={discoveryActionsBlocked}
          discoveryRefreshFailed={discoveryRefreshFailed}
          discoveryRefreshing={preview.isFetching}
          error={error}
          onOpenChange={setConfirmOpen}
          onRetryDiscovery={() => preview.refetch()}
          onDiscoveryFallbackFocus={focusDiscoveryResult}
          onConfirm={() => {
            if (actionsBlocked) return;
            run.mutate(undefined, {
              onSuccess: () => setConfirmOpen(false),
            });
          }}
        />
      ) : null}
    </section>
  );
}
