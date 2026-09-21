import type { ComponentType } from "react";
import { AlertCircle, AlertTriangle, CheckCircle2, Info } from "lucide-react";
import { useTranslation } from "react-i18next";
import { Button } from "@/shared/ui/Button";
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

export interface QuickCheckSignalItemProps {
  item: QuickCheckItem;
  /** The row's own next step. Omitted when the row is informational only. */
  action?: { label: string; disabled?: boolean; onSelect: () => void };
}

/**
 * One finding as one row.
 *
 * Title and detail share a line at wide widths and stack at narrow ones, so
 * the whole list stays scannable instead of one item being expanded while the
 * rest hide behind a disclosure. The action belongs on the row because that is
 * where the reader already is when they decide to act on it.
 */
export function QuickCheckSignalItem({
  item,
  action,
}: QuickCheckSignalItemProps) {
  const { t } = useTranslation();
  const meta = ITEM_META[item.status];
  const Icon = meta.icon;

  return (
    <li
      data-status={item.status}
      className="flex min-w-0 items-center gap-3 rounded-lg border border-hairline bg-layer-1 px-3 py-2.5"
    >
      <span
        className={`flex h-7 w-7 shrink-0 items-center justify-center rounded-md ${meta.surface}`}
      >
        <Icon className={`h-4 w-4 ${meta.color}`} aria-hidden="true" />
      </span>
      <div className="flex min-w-0 flex-1 flex-col gap-x-2 sm:flex-row sm:items-baseline">
        <p className="shrink-0 text-body font-medium text-content">
          {t(item.titleKey, item.values)}
        </p>
        <p className="min-w-0 truncate text-caption text-content-muted">
          {t(item.descriptionKey, item.values)}
        </p>
      </div>
      {action ? (
        <Button
          variant="secondary"
          size="sm"
          className="shrink-0"
          disabled={action.disabled}
          onClick={action.onSelect}
        >
          {action.label}
        </Button>
      ) : null}
    </li>
  );
}
