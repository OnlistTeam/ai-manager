import { AlertCircle, Route } from "lucide-react";
import { useState } from "react";
import { useTranslation } from "react-i18next";
import { useRoutingOverview } from "@/entities/routing";
import { LiveRoutingPanel } from "@/features/live-routing";
import { PrivacyStatusLine } from "@/features/routing-privacy";
import { ConfirmActionModal } from "@/features/tool-management";
import { toErrorCopy } from "@/shared/lib/nativeError";
import { Button } from "@/shared/ui/Button";
import { Card } from "@/shared/ui/Card";
import { DetectionStatus } from "@/shared/ui/DetectionStatus";
import { EmptyState } from "@/shared/ui/EmptyState";
import { SectionHeader } from "@/shared/ui/SectionHeader";
import { RoutingSummary } from "./RoutingSummary";
import { RoutingToolList } from "./RoutingToolList";
import { useRoutingActions } from "./useRoutingActions";

export interface RoutingPageProps {
  /** Opens the privacy section of the settings page. */
  onOpenPrivacySettings?: () => void;
}

/**
 * Local routing (ADR-0054): each tool is routed through AI Manager only when
 * the user turns it on, one row per tool, with the live requests below.
 */
export function RoutingPage({ onOpenPrivacySettings }: RoutingPageProps) {
  const { t } = useTranslation();
  const overview = useRoutingOverview();
  const actions = useRoutingActions();
  const [confirmingStop, setConfirmingStop] = useState(false);
  const errorCopy = actions.error ? toErrorCopy(actions.error) : null;
  const initiallyLoading = overview.isPending && !overview.isFetched;
  const unavailable = overview.isFetched && overview.data === undefined;
  const stale = overview.isError && overview.data !== undefined;

  return (
    <div
      role="region"
      aria-label={t("routing.title")}
      className="flex min-w-0 flex-col gap-4"
    >
      <SectionHeader
        as="h2"
        title={t("routing.title")}
        description={t("routing.description")}
      />

      {initiallyLoading ? (
        <DetectionStatus label={t("routing.loading")} />
      ) : null}

      {unavailable ? (
        <div role="alert">
          <EmptyState
            icon={Route}
            title={t("routing.error.title")}
            description={t("routing.error.description")}
            action={
              <Button
                loading={overview.isFetching}
                onClick={() => void overview.refetch()}
              >
                {t("routing.error.retry")}
              </Button>
            }
          />
        </div>
      ) : null}

      {stale ? (
        <Card
          role="alert"
          className="flex items-start gap-3 border-warning/25 bg-warning/5"
        >
          <AlertCircle
            className="mt-0.5 h-4 w-4 text-warning"
            aria-hidden="true"
          />
          <p className="min-w-0 flex-1 text-caption text-content-muted">
            {t("routing.stale")}
          </p>
          <Button
            size="sm"
            variant="secondary"
            onClick={() => void overview.refetch()}
          >
            {t("routing.error.retry")}
          </Button>
        </Card>
      ) : null}

      {errorCopy ? (
        <Card
          role="alert"
          className="flex items-start gap-3 border-danger/25 bg-danger/5"
        >
          <AlertCircle
            className="mt-0.5 h-4 w-4 text-danger"
            aria-hidden="true"
          />
          <div className="min-w-0">
            <p className="text-body font-medium text-content">
              {t("routing.actionError.title")}
            </p>
            <p className="mt-0.5 text-caption text-content-muted">
              {t(errorCopy.messageKey)}
            </p>
          </div>
        </Card>
      ) : null}

      {overview.data ? (
        <>
          <Card className="flex flex-col gap-2">
            <RoutingSummary
              overview={overview.data}
              busy={actions.busy}
              onStop={() => {
                actions.reset();
                setConfirmingStop(true);
              }}
            />
            <PrivacyStatusLine
              onOpenSettings={onOpenPrivacySettings}
              className="border-t border-hairline pt-2"
            />
          </Card>
          <RoutingToolList
            targets={overview.data.targets}
            actions={actions}
            disabled={actions.busy || confirmingStop}
          />
          {overview.data.running ? <LiveRoutingPanel /> : null}
        </>
      ) : null}

      <ConfirmActionModal
        open={confirmingStop}
        onOpenChange={(open) => {
          if (!open && !actions.busy) {
            actions.reset();
            setConfirmingStop(false);
          }
        }}
        title={t("routing.stop.title")}
        description={t("routing.stop.description")}
        confirmLabel={t("routing.stop.confirm")}
        confirmTone="danger"
        busy={actions.busy}
        error={actions.error}
        onConfirm={() => actions.stopAll(() => setConfirmingStop(false))}
      />
    </div>
  );
}
