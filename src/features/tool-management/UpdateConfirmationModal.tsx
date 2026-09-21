import { AlertTriangle, Loader2, RotateCw } from "lucide-react";
import {
  useEffect,
  useMemo,
  useRef,
  type ReactNode,
  type RefObject,
} from "react";
import { useTranslation } from "react-i18next";
import type { Tool, ToolUpdatePreview, ToolUpdateReadyPreview } from "@/native";
import { Button } from "@/shared/ui/Button";
import { Modal } from "@/shared/ui/Modal";
import { ToolActionError } from "./ToolActionError";
import { UpdatePreviewDetails } from "./UpdatePreviewDetails";

export interface UpdateConfirmationModalProps {
  open: boolean;
  tools: readonly Tool[];
  previews: readonly ToolUpdatePreview[] | undefined;
  loading: boolean;
  refreshing: boolean;
  previewError: Error | null;
  /**
   * Handoff in progress: the product is queuing this update into "Activity".
   * It isn't a write operation, so the dialog can still be closed — see the
   * note at `Modal` below.
   */
  submitting: boolean;
  mutationError?: Error | null;
  actionPaused?: boolean;
  bulk?: boolean;
  confirmLabel?: string;
  notice?: ReactNode;
  returnFocusFallbackRef?: RefObject<HTMLElement>;
  onOpenChange: (open: boolean) => void;
  onRefresh: () => void;
  onConfirm: (ready: readonly ToolUpdateReadyPreview[]) => void;
}

function readyPreviews(previews: readonly ToolUpdatePreview[] | undefined) {
  return (previews ?? []).flatMap((item) =>
    item.state === "ready" ? [item.preview] : [],
  );
}

function isStaleError(error: Error | null | undefined) {
  return (
    error !== null &&
    error !== undefined &&
    "code" in error &&
    error.code === "UPDATE_PREVIEW_STALE"
  );
}

export function UpdateConfirmationModal({
  open,
  tools,
  previews,
  loading,
  refreshing,
  previewError,
  submitting,
  mutationError = null,
  actionPaused = false,
  bulk = false,
  confirmLabel: confirmLabelOverride,
  notice,
  returnFocusFallbackRef,
  onOpenChange,
  onRefresh,
  onConfirm,
}: UpdateConfirmationModalProps) {
  const { t } = useTranslation();
  const cancelRef = useRef<HTMLButtonElement>(null);
  const confirmRef = useRef<HTMLButtonElement>(null);
  const refreshRef = useRef<HTMLButtonElement>(null);
  const ready = useMemo(() => readyPreviews(previews), [previews]);
  const stale = isStaleError(mutationError);
  const single = !bulk && tools.length === 1;
  const blockedCount = Math.max(0, tools.length - ready.length);
  const needsRefresh = Boolean(previewError) || stale || ready.length === 0;
  const confirmDisabled =
    loading ||
    refreshing ||
    Boolean(previewError) ||
    actionPaused ||
    submitting ||
    stale ||
    ready.length === 0;
  const refreshDisabled = loading || submitting || actionPaused;
  const initialFocusRef = confirmDisabled
    ? needsRefresh && !refreshDisabled
      ? refreshRef
      : cancelRef
    : confirmRef;
  const previousState = useRef({ loading, refreshing, confirmDisabled });
  useEffect(() => {
    const previous = previousState.current;
    previousState.current = { loading, refreshing, confirmDisabled };
    if (!open) return;

    const active = document.activeElement;
    const defaultFocus =
      active === cancelRef.current ||
      active === refreshRef.current ||
      active === document.body ||
      !(active instanceof HTMLElement) ||
      !active.isConnected;
    if (!defaultFocus) return;

    if (previous.loading && !loading && needsRefresh && !refreshDisabled) {
      refreshRef.current?.focus();
      return;
    }
    if (
      ((previous.loading && !loading) ||
        (previous.refreshing && !refreshing) ||
        (previous.confirmDisabled && !confirmDisabled)) &&
      !confirmDisabled
    ) {
      confirmRef.current?.focus();
    }
  }, [
    confirmDisabled,
    loading,
    needsRefresh,
    open,
    refreshDisabled,
    refreshing,
  ]);
  const title = single
    ? t("tools.confirm.update.title", { name: tools[0]?.name ?? "" })
    : t("home.updateAll.reviewTitle", { count: tools.length });
  const description = single
    ? t("tools.updatePreview.singleDescription")
    : t("home.updateAll.previewSummary", {
        ready: ready.length,
        skipped: blockedCount,
      });
  const confirmLabel =
    confirmLabelOverride ??
    (single
      ? t("tools.confirm.update.confirm")
      : t("home.updateAll.confirm", { count: ready.length }));

  return (
    <Modal
      open={open}
      // Deliberately kept closable during the handoff. Between clicking
      // "Update" and the task appearing in "Activity", the product has to
      // re-detect the local version and check the registry for the latest
      // one again; on a constrained network these few seconds can turn
      // into a dozen, and `dismissible={false}` would simultaneously
      // disable the X, Escape, click-outside, and Cancel — boxing the user
      // in on all sides. Closing the dialog doesn't undo the handoff: the
      // task still gets queued into "Activity" and the page still shows
      // it. What's actually irreversible are editing dialogs — that's
      // where the lock is actually needed.
      onOpenChange={onOpenChange}
      initialFocusRef={initialFocusRef}
      returnFocusFallbackRef={returnFocusFallbackRef}
      title={title}
      description={description}
      size="lg"
      footer={
        <>
          <Button
            ref={cancelRef}
            variant="secondary"
            disabled={submitting}
            onClick={() => onOpenChange(false)}
          >
            {t("ds.action.cancel")}
          </Button>
          {needsRefresh || refreshing ? (
            <Button
              ref={refreshRef}
              variant="secondary"
              loading={refreshing}
              disabled={refreshDisabled}
              onClick={onRefresh}
            >
              <RotateCw className="h-4 w-4" aria-hidden="true" />
              {t("tools.updatePreview.recheck")}
            </Button>
          ) : null}
          <Button
            ref={confirmRef}
            disabled={confirmDisabled}
            loading={submitting}
            onClick={() => onConfirm(ready)}
          >
            {confirmLabel}
          </Button>
        </>
      }
    >
      <div className="space-y-4">
        {notice}
        {loading ? (
          <div
            role="status"
            aria-label={t("tools.updatePreview.loading")}
            className="flex min-h-32 flex-col items-center justify-center gap-3 rounded-xl border border-hairline bg-layer-1 px-4 py-8 text-center"
          >
            <Loader2
              className="h-5 w-5 text-brand motion-safe:animate-spin"
              aria-hidden="true"
            />
            <p className="text-body font-medium text-content">
              {t("tools.updatePreview.loading")}
            </p>
            <p className="max-w-sm text-caption text-content-muted">
              {t("tools.updatePreview.loadingHint")}
            </p>
          </div>
        ) : previewError ? (
          <div
            role="alert"
            aria-label={t("tools.updatePreview.error.title")}
            className="flex items-start gap-3 rounded-xl border border-warning/30 bg-warning/5 p-4"
          >
            <AlertTriangle
              className="mt-0.5 h-5 w-5 shrink-0 text-warning"
              aria-hidden="true"
            />
            <div>
              <p className="text-body font-medium text-content">
                {t("tools.updatePreview.error.title")}
              </p>
              <p className="mt-1 text-caption leading-5 text-content-muted">
                {t("tools.updatePreview.error.description")}
              </p>
            </div>
          </div>
        ) : previews ? (
          <UpdatePreviewDetails tools={tools} previews={previews} />
        ) : null}

        {refreshing && previews ? (
          <p
            role="status"
            aria-label={t("tools.updatePreview.refreshing")}
            className="flex items-center gap-2 text-caption text-content-muted"
          >
            <Loader2
              className="h-4 w-4 motion-safe:animate-spin"
              aria-hidden="true"
            />
            {t("tools.updatePreview.refreshing")}
          </p>
        ) : null}

        {actionPaused ? (
          <p className="rounded-lg border border-warning/25 bg-warning/5 px-3 py-2 text-caption text-content-muted">
            {t("tools.updatePreview.paused")}
          </p>
        ) : null}

        {mutationError ? (
          <ToolActionError error={mutationError} className="mt-3" />
        ) : null}
      </div>
    </Modal>
  );
}
