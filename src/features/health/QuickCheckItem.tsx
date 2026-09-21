import type { ComponentType } from "react";
import { AlertCircle, AlertTriangle, CheckCircle2, Info } from "lucide-react";
import { useTranslation } from "react-i18next";
import type { QuickCheckItem, QuickCheckItemStatus } from "./quickCheck";

const ITEM_META: Record<
  QuickCheckItemStatus,
  {
    icon: ComponentType<{ className?: string }>;
    color: string;
    surface: string;
  }
> = {
  ready: {
    icon: CheckCircle2,
    color: "text-success",
    surface: "bg-success/10",
  },
  attention: {
    icon: AlertTriangle,
    color: "text-warning",
    surface: "bg-warning/10",
  },
  action: {
    icon: AlertCircle,
    color: "text-danger",
    surface: "bg-danger/10",
  },
  info: {
    icon: Info,
    color: "text-content-muted",
    surface: "bg-layer-1",
  },
};

export function QuickCheckSignalItem({ item }: { item: QuickCheckItem }) {
  const { t } = useTranslation();
  const meta = ITEM_META[item.status];
  const Icon = meta.icon;

  return (
    <li
      data-status={item.status}
      className="flex min-w-0 gap-3 rounded-lg border border-hairline bg-layer-1 p-3"
    >
      <span
        className={`flex h-8 w-8 shrink-0 items-center justify-center rounded-lg ${meta.surface}`}
      >
        <Icon className={`h-4 w-4 ${meta.color}`} aria-hidden="true" />
      </span>
      <div className="min-w-0 flex-1">
        <p className="text-body font-medium text-content">
          {t(item.titleKey, item.values)}
        </p>
        <p className="mt-0.5 text-caption text-content-muted">
          {t(item.descriptionKey, item.values)}
        </p>
      </div>
    </li>
  );
}
