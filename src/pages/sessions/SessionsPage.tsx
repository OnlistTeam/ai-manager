import { AlertCircle, MessagesSquare, ShieldCheck } from "lucide-react";
import { useEffect, useMemo, useState } from "react";
import { useTranslation } from "react-i18next";
import {
  useResumeSession,
  useSessions,
  useSessionThread,
} from "@/entities/session";
import type { ToolId } from "@/entities/tool";
import { Button } from "@/shared/ui/Button";
import { DetectionStatus } from "@/shared/ui/DetectionStatus";
import { EmptyState } from "@/shared/ui/EmptyState";
import { SectionHeader } from "@/shared/ui/SectionHeader";
import { TooltipProvider } from "@/shared/ui/Tooltip";
import { SessionListPanel } from "./SessionListPanel";
import { SessionThreadPanel } from "./SessionThreadPanel";
import { SessionsToolbar } from "./SessionsToolbar";
import { useDebouncedValue } from "./useDebouncedValue";

/** The backend caches results after a single scan; here we just need to hold back rapid keystrokes. */
const SEARCH_DEBOUNCE_MS = 200;

export interface SessionsPageProps {
  tool: ToolId | null;
  showHeader?: boolean;
}

export function SessionsPage({ tool, showHeader = true }: SessionsPageProps) {
  const { t } = useTranslation();
  const [input, setInput] = useState("");
  const query = useDebouncedValue(input.trim(), SEARCH_DEBOUNCE_MS);
  const [selectedReference, setSelectedReference] = useState<string | null>(
    null,
  );
  const sessions = useSessions(query, tool);
  const thread = useSessionThread(selectedReference);
  const resume = useResumeSession();
  const { reset: resetResume } = resume;
  const selected = useMemo(
    () =>
      sessions.data?.items.find(
        (item) => item.reference === selectedReference,
      ) ?? null,
    [selectedReference, sessions.data?.items],
  );

  useEffect(() => {
    if (selectedReference !== null && selected === null && sessions.data) {
      setSelectedReference(null);
    }
  }, [selected, selectedReference, sessions.data]);

  useEffect(() => {
    setSelectedReference(null);
    resetResume();
  }, [tool, resetResume]);

  function select(reference: string): void {
    resetResume();
    setSelectedReference(reference);
  }

  return (
    <TooltipProvider delayDuration={200} skipDelayDuration={100}>
      <div
        role="region"
        aria-label={t("sessions.title")}
        className="flex min-w-0 flex-col gap-6"
      >
        {showHeader ? (
          <SectionHeader
            as="h2"
            title={t("sessions.title")}
            description={t("sessions.description")}
          />
        ) : null}
        {sessions.data ? (
          <p className="flex flex-wrap items-center gap-x-2 gap-y-1 text-caption text-content-muted">
            <ShieldCheck
              className="h-3.5 w-3.5 shrink-0 text-success"
              aria-hidden="true"
            />
            <span>{t("sessions.hero.local")}</span>
            <span aria-hidden="true">·</span>
            <span>
              {t("sessions.hero.metric.visible")} {sessions.data.items.length}
            </span>
            <span aria-hidden="true">·</span>
            <span>
              {t("sessions.hero.metric.tools")}{" "}
              {new Set(sessions.data.items.map((item) => item.tool)).size}
            </span>
            <span aria-hidden="true">·</span>
            <span>
              {t("sessions.hero.metric.resumable")}{" "}
              {sessions.data.items.filter((item) => item.resumable).length}
            </span>
          </p>
        ) : null}
        {sessions.data?.limited ? (
          <p className="text-caption text-warning">
            {t("sessions.hero.limited", { count: sessions.data.totalCount })}
          </p>
        ) : null}
        <SessionsToolbar
          input={input}
          refreshing={sessions.isFetching}
          onInput={setInput}
          onRefresh={() => void sessions.refetch()}
        />

        {sessions.isPending ? (
          <DetectionStatus label={t("sessions.loading")} />
        ) : null}

        {sessions.isError && !sessions.data ? (
          <EmptyState
            icon={AlertCircle}
            title={t("sessions.error.title")}
            description={t("sessions.error.description")}
            action={
              <Button onClick={() => void sessions.refetch()}>
                {t("sessions.refresh")}
              </Button>
            }
          />
        ) : null}

        {sessions.data?.items.length === 0 ? (
          <EmptyState
            icon={MessagesSquare}
            title={t("sessions.empty.title")}
            description={t(
              query ? "sessions.empty.search" : "sessions.empty.description",
            )}
          />
        ) : null}

        {sessions.data && sessions.data.items.length > 0 ? (
          selectedReference ? (
            <div className="grid min-w-0 gap-4 lg:grid-cols-[minmax(320px,0.78fr)_minmax(0,1.22fr)] lg:items-start">
              <SessionListPanel
                list={sessions.data}
                selected={selectedReference}
                onSelect={select}
              />
              <SessionThreadPanel
                selected={selected}
                thread={thread.data}
                loading={thread.isPending && selectedReference !== null}
                error={thread.error}
                resuming={resume.isPending}
                resumed={resume.isSuccess}
                resumeError={resume.error}
                onRetry={() => void thread.refetch()}
                onResume={() => {
                  if (selectedReference !== null) {
                    resume.reset();
                    resume.mutate(selectedReference);
                  }
                }}
              />
            </div>
          ) : (
            <SessionListPanel
              list={sessions.data}
              selected={selectedReference}
              onSelect={select}
            />
          )
        ) : null}
      </div>
    </TooltipProvider>
  );
}
