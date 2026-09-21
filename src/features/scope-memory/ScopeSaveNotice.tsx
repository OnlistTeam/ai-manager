import { AlertTriangle, Loader2 } from "lucide-react";
import { Button } from "@/shared/ui/Button";

export interface ScopeSaveNoticeProps {
  saving: boolean;
  failed: boolean;
  savingLabel: string;
  errorTitle: string;
  errorDescription: string;
  retryLabel: string;
  retryDisabled?: boolean;
  onRetry: () => void;
}

/**
 * Shared feedback for page-scope preferences. The owning page supplies the
 * product copy and persistence behavior; this component only keeps the
 * recoverable state visually and accessibly consistent.
 */
export function ScopeSaveNotice({
  saving,
  failed,
  savingLabel,
  errorTitle,
  errorDescription,
  retryLabel,
  retryDisabled = false,
  onRetry,
}: ScopeSaveNoticeProps) {
  if (saving) {
    return (
      <p
        role="status"
        aria-label={savingLabel}
        aria-live="polite"
        className="inline-flex items-center gap-2 text-caption text-content-muted"
      >
        <Loader2
          className="h-3.5 w-3.5 motion-safe:animate-spin"
          aria-hidden="true"
        />
        {savingLabel}
      </p>
    );
  }

  if (!failed) return null;

  return (
    <div
      role="alert"
      aria-label={errorTitle}
      className="flex flex-col gap-3 rounded-lg border border-warning/30 bg-warning/10 px-4 py-3 sm:flex-row sm:items-center sm:justify-between"
    >
      <div className="flex min-w-0 items-start gap-3">
        <AlertTriangle
          className="mt-0.5 h-4 w-4 shrink-0 text-warning"
          aria-hidden="true"
        />
        <div className="min-w-0">
          <p className="text-body font-medium text-content">{errorTitle}</p>
          <p className="mt-0.5 text-caption leading-5 text-content-muted">
            {errorDescription}
          </p>
        </div>
      </div>
      <Button
        variant="secondary"
        size="sm"
        className="self-start sm:self-auto"
        disabled={retryDisabled}
        onClick={onRetry}
      >
        {retryLabel}
      </Button>
    </div>
  );
}
