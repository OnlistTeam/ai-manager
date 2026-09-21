import { AlertTriangle, RefreshCw } from "lucide-react";
import type { Ref } from "react";
import { Button } from "./Button";

export interface InventoryRefreshNoticeProps {
  title: string;
  description: string;
  actionLabel: string;
  refreshing: boolean;
  retryButtonRef?: Ref<HTMLButtonElement>;
  onRetry: () => void;
}

/**
 * Keeps the last authoritative inventory readable while saying that a fresh
 * read failed. Purely presentational: which inventory, and what the retry
 * does, is the caller's business.
 */
export function InventoryRefreshNotice({
  title,
  description,
  actionLabel,
  refreshing,
  retryButtonRef,
  onRetry,
}: InventoryRefreshNoticeProps) {
  return (
    <aside
      role="alert"
      aria-label={title}
      aria-busy={refreshing || undefined}
      className="flex min-w-0 flex-col gap-3 rounded-lg border border-warning/30 bg-warning/10 px-4 py-3 sm:flex-row sm:items-center sm:justify-between"
    >
      <div className="flex min-w-0 items-start gap-3">
        <AlertTriangle
          className="mt-0.5 h-4 w-4 shrink-0 text-warning"
          aria-hidden="true"
        />
        <div className="min-w-0">
          <p className="text-body font-medium text-content">{title}</p>
          <p className="mt-0.5 text-caption leading-5 text-content-muted">
            {description}
          </p>
        </div>
      </div>
      <Button
        ref={retryButtonRef}
        variant="secondary"
        size="sm"
        className="self-start sm:self-auto"
        loading={refreshing}
        onClick={onRetry}
      >
        <RefreshCw className="h-4 w-4" aria-hidden="true" />
        {actionLabel}
      </Button>
    </aside>
  );
}
