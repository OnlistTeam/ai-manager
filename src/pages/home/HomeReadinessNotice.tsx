import { AlertTriangle, LoaderCircle, Stethoscope } from "lucide-react";
import type { Ref } from "react";
import { useTranslation } from "react-i18next";
import { Button } from "@/shared/ui/Button";
import { STATUS_LINE_CLASS } from "./HomeStatusLine";

/** The status line before the first check has an answer. */
export function HomeStatusChecking() {
  const { t } = useTranslation();

  return (
    <div
      role="status"
      aria-label={t("home.status.checking")}
      className={STATUS_LINE_CLASS}
    >
      <LoaderCircle
        className="h-4 w-4 shrink-0 text-content-muted motion-safe:animate-spin"
        aria-hidden="true"
      />
      <h1 className="text-body font-medium text-content">
        {t("home.status.checking")}
      </h1>
    </div>
  );
}

interface HomeUnavailableStateProps {
  retrying: boolean;
  retryButtonRef: Ref<HTMLButtonElement>;
  onRetry: () => void;
}

/** Keeps the initial Home failure and its retry context stable while checking. */
export function HomeUnavailableState({
  retrying,
  retryButtonRef,
  onRetry,
}: HomeUnavailableStateProps) {
  const { t } = useTranslation();

  return (
    <div
      role="alert"
      aria-labelledby="home-unavailable-error-title"
      aria-busy={retrying || undefined}
      className={STATUS_LINE_CLASS}
    >
      <Stethoscope
        className="h-4 w-4 shrink-0 text-danger"
        aria-hidden="true"
      />
      <h1
        id="home-unavailable-error-title"
        className="text-body font-medium text-content"
      >
        {t("home.error.title")}
      </h1>
      <p className="min-w-0 text-caption text-content-muted">
        {t("home.error.description")}
      </p>
      <Button
        ref={retryButtonRef}
        variant="secondary"
        size="xs"
        className="ml-auto"
        loading={retrying}
        onClick={onRetry}
      >
        {t("home.refreshError.action")}
      </Button>
    </div>
  );
}

interface HomeRefreshNoticeProps {
  retrying: boolean;
  retryButtonRef: Ref<HTMLButtonElement>;
  onRetry: () => void;
}

/** Labels retained Home data and provides the one authoritative retry. */
export function HomeRefreshNotice({
  retrying,
  retryButtonRef,
  onRetry,
}: HomeRefreshNoticeProps) {
  const { t } = useTranslation();

  return (
    <aside
      role="alert"
      aria-label={t("home.refreshError.title")}
      aria-busy={retrying || undefined}
      className="flex min-w-0 flex-col gap-3 rounded-xl border border-warning/30 bg-warning/10 px-4 py-3 sm:flex-row sm:items-center"
    >
      <div className="flex min-w-0 items-start gap-3">
        <AlertTriangle
          className="mt-0.5 h-4 w-4 shrink-0 text-warning"
          aria-hidden="true"
        />
        <div className="min-w-0">
          <p className="text-body font-medium text-content">
            {t("home.refreshError.title")}
          </p>
          <p className="mt-0.5 text-caption leading-5 text-content-muted">
            {t("home.refreshError.description")}
          </p>
        </div>
      </div>
      <Button
        ref={retryButtonRef}
        variant="secondary"
        loading={retrying}
        className="shrink-0 self-start sm:ml-auto"
        onClick={onRetry}
      >
        {t("home.refreshError.action")}
      </Button>
    </aside>
  );
}

/** Explains why a confirmation that was already open has paused. */
export function HomeActionPausedNotice() {
  const { t } = useTranslation();

  return (
    <div
      role="alert"
      aria-label={t("home.refreshError.title")}
      className="mt-3 flex min-w-0 items-start gap-3 rounded-lg border border-warning/30 bg-warning/10 p-3"
    >
      <AlertTriangle
        className="mt-0.5 h-4 w-4 shrink-0 text-warning"
        aria-hidden="true"
      />
      <div className="min-w-0">
        <p className="text-body font-medium text-content">
          {t("home.refreshError.title")}
        </p>
        <p className="mt-0.5 text-caption leading-5 text-content-muted">
          {t("home.refreshError.description")}
        </p>
      </div>
    </div>
  );
}

/** Keeps an already-open confirmation honest during a background recheck. */
export function HomeActionCheckingNotice() {
  const { t } = useTranslation();

  return (
    <div
      role="status"
      aria-label={t("home.refreshing.title")}
      aria-busy="true"
      className="mt-3 flex min-w-0 items-start gap-3 rounded-lg border border-brand/20 bg-brand/5 p-3"
    >
      <LoaderCircle
        className="mt-0.5 h-4 w-4 shrink-0 text-brand motion-safe:animate-spin"
        aria-hidden="true"
      />
      <div className="min-w-0">
        <p className="text-body font-medium text-content">
          {t("home.refreshing.title")}
        </p>
        <p className="mt-0.5 text-caption leading-5 text-content-muted">
          {t("home.refreshing.description")}
        </p>
      </div>
    </div>
  );
}
