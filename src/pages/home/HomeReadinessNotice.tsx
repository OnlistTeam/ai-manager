import { AlertTriangle, LoaderCircle, Stethoscope } from "lucide-react";
import type { Ref } from "react";
import { useTranslation } from "react-i18next";
import { TERMINAL_TURNTABLE } from "./terminalModel";
import { Button } from "@/shared/ui/Button";
import { Card } from "@/shared/ui/Card";
import { SpatialScene } from "@/shared/ui/SpatialScene";
import { useSpatialPointer } from "@/shared/ui/useSpatialPointer";

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
  // What's rendered on stage is a turntable, and the pointer needs to know
  // that too. Without this parameter, yaw would fall back to the
  // "CSS-rotate a flat image" path, and stacking that on top of the
  // turntable's own frame would rotate the same object twice.
  const pointer = useSpatialPointer<HTMLDivElement>(TERMINAL_TURNTABLE);

  return (
    <Card
      ref={pointer.ref}
      role="alert"
      aria-labelledby="home-unavailable-error-title"
      aria-busy={retrying || undefined}
      padding="none"
      data-model="environment"
      data-tone="danger"
      data-spatial-stage=""
      className="environment-hero spatial-page-hero relative isolate min-h-[430px] min-w-0 overflow-hidden lg:min-h-[350px]"
    >
      <span aria-hidden="true" className="spatial-page-hero__wash" />
      <span aria-hidden="true" className="spatial-page-hero__lines" />

      <div className="environment-hero__layout spatial-page-hero__layout min-h-[430px] lg:min-h-[350px]">
        <SpatialScene
          icon={Stethoscope}
          model="environment"
          turntable={TERMINAL_TURNTABLE}
          tone="danger"
          size="hero"
          loading={retrying}
          className="spatial-page-hero__scene"
        />

        <div className="spatial-page-hero__content">
          <div className="spatial-page-hero__eyebrow">
            <Stethoscope
              className="h-4 w-4 shrink-0 text-danger"
              aria-hidden="true"
            />
            <span>{t("home.card.title")}</span>
            <span aria-hidden="true">·</span>
            <span>{t("home.error.title")}</span>
          </div>
          <h1
            id="home-unavailable-error-title"
            className="spatial-page-hero__title"
          >
            {t("home.error.title")}
          </h1>
          <p className="spatial-page-hero__description">
            {t("home.error.description")}
          </p>

          <Button
            ref={retryButtonRef}
            className="environment-hero__review mt-6"
            loading={retrying}
            onClick={onRetry}
          >
            {t("home.refreshError.action")}
          </Button>
        </div>
      </div>
    </Card>
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
