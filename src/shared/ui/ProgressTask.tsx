import * as React from "react";
import { AlertCircle, Ban, CheckCircle2, Clock, Loader2 } from "lucide-react";
import { useTranslation } from "react-i18next";
import type { OperationStatus } from "@/native/schemas/operation";
import { cn } from "./cn";
import { Badge, type BadgeTone } from "./Badge";
import { Button } from "./Button";
import { Progress } from "./Progress";

const STATUS_META: Record<
  OperationStatus,
  {
    tone: BadgeTone;
    icon: React.ComponentType<{ className?: string }>;
    labelKey: string;
    active: boolean;
  }
> = {
  queued: {
    tone: "neutral",
    icon: Clock,
    labelKey: "ds.task.queued",
    active: true,
  },
  running: {
    tone: "brand",
    icon: Loader2,
    labelKey: "ds.task.running",
    active: true,
  },
  success: {
    tone: "success",
    icon: CheckCircle2,
    labelKey: "ds.task.success",
    active: false,
  },
  failed: {
    tone: "danger",
    icon: AlertCircle,
    labelKey: "ds.task.failed",
    active: false,
  },
  cancelled: {
    tone: "neutral",
    icon: Ban,
    labelKey: "ds.task.cancelled",
    active: false,
  },
};

export interface ProgressTaskProps {
  name: string;
  progress: number;
  status: OperationStatus;
  /** Human-readable step, e.g. "Downloading" — never raw shell output (§30). */
  detail?: string;
  /** Override when several progress bars share a surface and need unique names. */
  progressLabel?: string;
  onCancel?: () => void;
  cancelPending?: boolean;
  className?: string;
}

export function ProgressTask({
  name,
  progress,
  status,
  detail,
  progressLabel,
  onCancel,
  cancelPending = false,
  className,
}: ProgressTaskProps) {
  const { t } = useTranslation();
  const meta = STATUS_META[status];
  const percent = Number.isFinite(progress)
    ? Math.min(100, Math.max(0, Math.round(progress)))
    : 0;

  return (
    <div className={cn("flex flex-col gap-2 py-3", className)}>
      <div className="flex flex-wrap items-start gap-x-3 gap-y-1.5">
        <span className="min-w-0 flex-[1_1_12rem] break-words text-body text-content">
          {name}
        </span>
        <div className="ml-auto flex shrink-0 items-center gap-2">
          {meta.active ? (
            <span className="text-mono-sm font-mono text-content-muted">
              {percent}%
            </span>
          ) : null}
          <Badge tone={meta.tone} icon={meta.icon}>
            {t(meta.labelKey)}
          </Badge>
          {meta.active && onCancel ? (
            <Button
              variant="ghost"
              size="sm"
              loading={cancelPending}
              disabled={cancelPending}
              onClick={onCancel}
            >
              {t("ds.action.cancel")}
            </Button>
          ) : null}
        </div>
      </div>
      {meta.active ? (
        <Progress
          value={percent}
          label={progressLabel ?? t("ds.task.progressLabel")}
        />
      ) : null}
      {detail ? (
        <p className="text-caption text-content-muted">{detail}</p>
      ) : null}
    </div>
  );
}
