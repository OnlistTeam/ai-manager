import { AlertTriangle, Ban, Waypoints } from "lucide-react";
import type { Ref } from "react";
import { useTranslation } from "react-i18next";
import { Button } from "@/shared/ui/Button";
import { EmptyState } from "@/shared/ui/EmptyState";
import {
  ConnectionProfileNotice,
  RecoverableNotice,
} from "./ConnectionProfileNotice";
import { ServicesSkeleton } from "./ServicesSkeleton";

export interface ServicesUnavailableStateProps {
  refreshing: boolean;
  retryButtonRef: Ref<HTMLButtonElement>;
  onRetry: () => void;
}

/** Keeps the initial read failure and its one retry control mounted while retrying. */
export function ServicesUnavailableState({
  refreshing,
  retryButtonRef,
  onRetry,
}: ServicesUnavailableStateProps) {
  const { t } = useTranslation();

  return (
    <div
      role="alert"
      aria-label={t("services.error.title")}
      aria-busy={refreshing || undefined}
    >
      <EmptyState
        icon={Waypoints}
        title={t("services.error.title")}
        description={t("services.error.description")}
        action={
          <Button ref={retryButtonRef} loading={refreshing} onClick={onRetry}>
            {t("services.refresh")}
          </Button>
        }
      />
    </div>
  );
}

export interface ServicesRefreshNoticeProps {
  refreshing: boolean;
  onRetry: () => void;
}

/** Keeps the last saved service inventory visible until a fresh read succeeds. */
export function ServicesRefreshNotice({
  refreshing,
  onRetry,
}: ServicesRefreshNoticeProps) {
  const { t } = useTranslation();

  return (
    <aside
      role="alert"
      aria-label={t("services.refreshError.title")}
      aria-busy={refreshing || undefined}
      className="flex min-w-0 items-start gap-3 rounded-lg border border-warning/30 bg-warning/10 px-4 py-3"
    >
      <AlertTriangle
        className="mt-0.5 h-4 w-4 shrink-0 text-warning"
        aria-hidden="true"
      />
      <div className="min-w-0 flex-1">
        <p className="text-body font-medium text-content">
          {t("services.refreshError.title")}
        </p>
        <p className="mt-0.5 text-caption leading-5 text-content-muted">
          {t("services.refreshError.description")}
        </p>
      </div>
      <Button
        size="sm"
        variant="secondary"
        loading={refreshing}
        onClick={onRetry}
      >
        {t("services.refresh")}
      </Button>
    </aside>
  );
}

export interface ServicesInventoryStatusProps {
  initiallyLoading: boolean;
  unavailable: boolean;
  refreshFailed: boolean;
  providerRefreshing: boolean;
  connectionUnavailable: boolean;
  connectionRefreshing: boolean;
  effectiveUnavailable: boolean;
  effectiveRefreshing: boolean;
  shellNotInspected: boolean;
  retryButtonRef: Ref<HTMLButtonElement>;
  onProviderRetry: () => void;
  onConnectionRetry: () => void;
  onEffectiveRetry: () => void;
}

/** Presents mutually compatible loading and recovery states above retained content. */
export function ServicesInventoryStatus({
  initiallyLoading,
  unavailable,
  refreshFailed,
  providerRefreshing,
  connectionUnavailable,
  connectionRefreshing,
  effectiveUnavailable,
  effectiveRefreshing,
  shellNotInspected,
  retryButtonRef,
  onProviderRetry,
  onConnectionRetry,
  onEffectiveRetry,
}: ServicesInventoryStatusProps) {
  const { t } = useTranslation();

  return (
    <>
      {initiallyLoading ? (
        <ServicesSkeleton label={t("services.loading")} />
      ) : null}
      {unavailable ? (
        <ServicesUnavailableState
          refreshing={providerRefreshing}
          retryButtonRef={retryButtonRef}
          onRetry={onProviderRetry}
        />
      ) : null}
      {refreshFailed ? (
        <ServicesRefreshNotice
          refreshing={providerRefreshing}
          onRetry={onProviderRetry}
        />
      ) : null}
      {connectionUnavailable ? (
        <ConnectionProfileNotice
          refreshing={connectionRefreshing}
          onRetry={onConnectionRetry}
        />
      ) : null}
      {effectiveUnavailable ? (
        <RecoverableNotice
          title={t("services.effective.errorTitle")}
          description={t("services.effective.errorDescription")}
          retryLabel={t("services.effective.refresh")}
          refreshing={effectiveRefreshing}
          onRetry={onEffectiveRetry}
        />
      ) : null}
      {shellNotInspected ? (
        <RecoverableNotice
          title={t("services.effective.shellNotInspectedTitle")}
          description={t("services.effective.shellNotInspected")}
        />
      ) : null}
    </>
  );
}

export function ServicesEmptyInventory() {
  const { t } = useTranslation();

  return (
    <EmptyState
      icon={Waypoints}
      title={t("services.empty.title")}
      description={t("services.empty.description")}
    />
  );
}

export function ServicesUnsupportedScope({ toolName }: { toolName: string }) {
  const { t } = useTranslation();

  return (
    <EmptyState
      icon={Ban}
      title={t("services.scopeUnsupportedTitle", { tool: toolName })}
      description={t("services.scopeUnsupportedDescription", {
        tool: toolName,
      })}
    />
  );
}
