import { AlertCircle, MessageSquareText, Play } from "lucide-react";
import { useState } from "react";
import { useTranslation } from "react-i18next";
import type { SessionSummary, SessionThread } from "@/entities/session";
import { toErrorCopy } from "@/shared/lib/nativeError";
import { Badge } from "@/shared/ui/Badge";
import { Button } from "@/shared/ui/Button";
import { Card } from "@/shared/ui/Card";
import { DetectionStatus } from "@/shared/ui/DetectionStatus";
import { EmptyState } from "@/shared/ui/EmptyState";
import { roleTone } from "./sessionFormat";
import { SessionResumeModal } from "./SessionResumeModal";
import { SessionRevealFolderButton } from "./SessionRevealFolderButton";

interface SessionThreadPanelProps {
  selected: SessionSummary | null;
  thread: SessionThread | undefined;
  loading: boolean;
  error: Error | null;
  resuming: boolean;
  resumed: boolean;
  resumeError: Error | null;
  onRetry: () => void;
  onResume: () => void;
}

export function SessionThreadPanel(props: SessionThreadPanelProps) {
  const { t } = useTranslation();
  const [resumeOpen, setResumeOpen] = useState(false);
  const readError = props.error ? toErrorCopy(props.error) : null;

  return (
    <Card padding="none" className="min-w-0 overflow-hidden rounded-xl">
      <div className="flex min-h-[65px] flex-wrap items-center justify-between gap-3 border-b border-hairline px-4 py-3">
        <div className="min-w-0">
          <h2 className="truncate text-heading text-content">
            {props.selected?.title ?? t("sessions.thread.title")}
          </h2>
          {props.selected ? (
            <p className="mt-0.5 text-caption text-content-muted">
              {t(`sessions.tool.${props.selected.tool}`)}
            </p>
          ) : null}
        </div>
        <div className="flex items-center gap-2">
          {props.selected?.resumable ? (
            <Button size="xs" onClick={() => setResumeOpen(true)}>
              <Play className="h-3.5 w-3.5" aria-hidden="true" />
              {t("sessions.resume.action")}
            </Button>
          ) : null}
          {props.selected ? (
            <SessionRevealFolderButton
              showLabel
              reference={props.selected.reference}
              title={props.selected.title}
            />
          ) : null}
        </div>
      </div>

      {!props.selected ? (
        <EmptyState
          icon={MessageSquareText}
          title={t("sessions.thread.emptyTitle")}
          description={t("sessions.thread.emptyDescription")}
          className="min-h-[420px]"
        />
      ) : null}

      {props.selected && props.loading ? (
        <DetectionStatus
          label={t("sessions.thread.loading")}
          className="m-5 min-h-[360px]"
        />
      ) : null}

      {readError ? (
        <EmptyState
          icon={AlertCircle}
          title={t("sessions.thread.errorTitle")}
          description={t(readError.messageKey)}
          action={
            <Button onClick={props.onRetry}>
              {t("sessions.thread.retry")}
            </Button>
          }
          className="min-h-[360px]"
        />
      ) : null}

      {props.thread ? (
        <div className="space-y-3 p-4">
          {props.thread.limited ? (
            <p className="text-caption text-content-muted">
              {t("sessions.thread.limited", { count: props.thread.totalCount })}
            </p>
          ) : null}
          {props.thread.messages.length === 0 ? (
            <EmptyState
              title={t("sessions.thread.noMessages")}
              className="min-h-[260px]"
            />
          ) : (
            <ol
              className="space-y-3"
              aria-label={t("sessions.thread.messages")}
            >
              {props.thread.messages.map((message, index) => (
                <li
                  key={`${message.timestamp ?? "none"}-${index}`}
                  className={`flex ${message.role === "user" ? "justify-end" : "justify-start"}`}
                >
                  <div className="max-w-[92%] rounded-xl border border-hairline bg-layer-1 px-3.5 py-3">
                    <Badge tone={roleTone(message.role)}>
                      {t(`sessions.role.${message.role}`)}
                    </Badge>
                    <p className="mt-2 whitespace-pre-wrap break-words text-body leading-6 text-content">
                      {message.content}
                    </p>
                    {message.truncated ? (
                      <p className="mt-2 text-caption text-warning">
                        {t("sessions.thread.messageTruncated")}
                      </p>
                    ) : null}
                  </div>
                </li>
              ))}
            </ol>
          )}
        </div>
      ) : null}

      <SessionResumeModal
        open={resumeOpen}
        command={props.thread?.resumeCommand ?? null}
        resuming={props.resuming}
        resumed={props.resumed}
        error={props.resumeError}
        onOpenChange={setResumeOpen}
        onResume={props.onResume}
      />
    </Card>
  );
}
