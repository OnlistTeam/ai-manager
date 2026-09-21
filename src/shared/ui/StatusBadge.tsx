import * as React from "react";
import { AlertCircle, AlertTriangle, CheckCircle2 } from "lucide-react";
import { useTranslation } from "react-i18next";
import { Badge, type BadgeTone } from "./Badge";

/** The only three health levels the product shows (spec §27). */
export type EnvironmentStatus = "ready" | "attention" | "action";

const STATUS_META: Record<
  EnvironmentStatus,
  {
    tone: BadgeTone;
    icon: React.ComponentType<{ className?: string }>;
    labelKey: string;
  }
> = {
  ready: { tone: "success", icon: CheckCircle2, labelKey: "ds.status.ready" },
  attention: {
    tone: "warning",
    icon: AlertTriangle,
    labelKey: "ds.status.attention",
  },
  action: { tone: "danger", icon: AlertCircle, labelKey: "ds.status.action" },
};

export interface StatusBadgeProps {
  status: EnvironmentStatus;
  className?: string;
}

export function StatusBadge({ status, className }: StatusBadgeProps) {
  const { t } = useTranslation();
  const meta = STATUS_META[status];
  return (
    <Badge
      tone={meta.tone}
      icon={meta.icon}
      className={className}
      data-status={status}
    >
      {t(meta.labelKey)}
    </Badge>
  );
}
