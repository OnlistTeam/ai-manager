import { AlertCircle, RefreshCw, Route } from "lucide-react";
import { useState } from "react";
import { useTranslation } from "react-i18next";
import {
  useAddRoutingProvider,
  useRemoveRoutingProvider,
  useRoutingOverview,
  useSetRoutingFailover,
  useSetRoutingTakeover,
  useStopAllRouting,
  useSwitchRoutingProvider,
  type RoutingTarget,
} from "@/entities/routing";
import { ConfirmActionModal } from "@/features/tool-management";
import { toErrorCopy } from "@/shared/lib/nativeError";
import { Button } from "@/shared/ui/Button";
import { Card } from "@/shared/ui/Card";
import { DetectionStatus } from "@/shared/ui/DetectionStatus";
import { EmptyState } from "@/shared/ui/EmptyState";
import { SectionHeader } from "@/shared/ui/SectionHeader";
import { RoutingSummary } from "./RoutingSummary";
import { RoutingTargetCard } from "./RoutingTargetCard";
import { RoutingTargetTabs } from "./RoutingTargetTabs";

type Confirmation =
  | { kind: "takeover"; target: RoutingTarget }
  | { kind: "stop" };

export function RoutingPage() {
  const { t } = useTranslation();
  const overview = useRoutingOverview();
  const takeover = useSetRoutingTakeover();
  const failover = useSetRoutingFailover();
  const add = useAddRoutingProvider();
  const remove = useRemoveRoutingProvider();
  const switchProvider = useSwitchRoutingProvider();
  const stop = useStopAllRouting();
  const [confirmation, setConfirmation] = useState<Confirmation | null>(null);
  const mutations = [takeover, failover, add, remove, switchProvider, stop];
  const busy = mutations.some((mutation) => mutation.isPending);
  const mutationError = mutations.find((mutation) => mutation.error)?.error;
  const errorCopy = mutationError ? toErrorCopy(mutationError) : null;
  const initiallyLoading = overview.isPending && !overview.isFetched;
  const unavailable = overview.isFetched && overview.data === undefined;
  const stale = overview.isError && overview.data !== undefined;
  const pendingTarget =
    confirmation?.kind === "takeover" ? confirmation.target : null;

  function resetMutationErrors(): void {
    for (const mutation of mutations) mutation.reset();
  }

  function confirm(): void {
    resetMutationErrors();
    if (confirmation?.kind === "takeover") {
      takeover.mutate(
        { tool: confirmation.target.tool, enabled: true },
        { onSuccess: () => setConfirmation(null) },
      );
    } else if (confirmation?.kind === "stop") {
      stop.mutate(undefined, { onSuccess: () => setConfirmation(null) });
    }
  }

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
        action={
          overview.data ? (
            <Button
              size="sm"
              variant="secondary"
              loading={overview.isFetching}
              disabled={busy}
              onClick={() => void overview.refetch()}
            >
              <RefreshCw className="h-4 w-4" aria-hidden="true" />
              {t("routing.refresh")}
            </Button>
          ) : null
        }
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
          <RoutingSummary
            overview={overview.data}
            busy={busy}
            onStop={() => {
              resetMutationErrors();
              setConfirmation({ kind: "stop" });
            }}
          />
          <RoutingTargetTabs
            targets={overview.data.targets}
            disabled={busy || confirmation !== null}
          >
            {(target) => (
              <RoutingTargetCard
                key={target.tool}
                target={target}
                busy={busy}
                onTakeoverChange={(enabled) => {
                  resetMutationErrors();
                  if (enabled) setConfirmation({ kind: "takeover", target });
                  else takeover.mutate({ tool: target.tool, enabled: false });
                }}
                onFailoverChange={(enabled) => {
                  resetMutationErrors();
                  failover.mutate({ tool: target.tool, enabled });
                }}
                onAdd={(providerId) => {
                  resetMutationErrors();
                  add.mutate({ tool: target.tool, providerId });
                }}
                onRemove={(providerId) => {
                  resetMutationErrors();
                  remove.mutate({ tool: target.tool, providerId });
                }}
                onSwitch={(providerId) => {
                  resetMutationErrors();
                  switchProvider.mutate({ tool: target.tool, providerId });
                }}
              />
            )}
          </RoutingTargetTabs>
        </>
      ) : null}

      <ConfirmActionModal
        open={confirmation !== null}
        onOpenChange={(open) => {
          if (!open && !busy) {
            resetMutationErrors();
            setConfirmation(null);
          }
        }}
        title={
          confirmation?.kind === "stop"
            ? t("routing.stop.title")
            : t("routing.takeover.confirmTitle", {
                name: pendingTarget
                  ? t(`routing.tool.${pendingTarget.tool}`)
                  : "",
              })
        }
        description={
          confirmation?.kind === "stop"
            ? t("routing.stop.description")
            : t("routing.takeover.confirmDescription")
        }
        confirmLabel={
          confirmation?.kind === "stop"
            ? t("routing.stop.confirm")
            : t("routing.takeover.confirm")
        }
        confirmTone={confirmation?.kind === "stop" ? "danger" : "primary"}
        busy={busy}
        error={mutationError ?? null}
        onConfirm={confirm}
      />
    </div>
  );
}
