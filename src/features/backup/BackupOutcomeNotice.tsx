import { AlertTriangle, CheckCircle2, MoveRight } from "lucide-react";
import { useTranslation } from "react-i18next";
import { Button } from "@/shared/ui/Button";

export type BackupOutcome =
  | { kind: "created" }
  | { kind: "exported" }
  | { kind: "imported"; toolsOutOfSync: boolean }
  | { kind: "renamed"; label: string }
  | { kind: "restored"; label: string; toolsOutOfSync: boolean }
  | { kind: "deleted"; label: string };

export interface BackupOutcomeNoticeProps {
  outcome: BackupOutcome;
  onReviewServices?: () => void;
}

/**
 * Backup writes must leave a result that remains visible after a toast window
 * would have passed. A restore sync warning is actionable and links directly
 * to the page where the user can resolve it.
 */
export function BackupOutcomeNotice({
  outcome,
  onReviewServices,
}: BackupOutcomeNoticeProps) {
  const { t } = useTranslation();
  const warning =
    (outcome.kind === "restored" || outcome.kind === "imported") &&
    outcome.toolsOutOfSync;
  const Icon = warning ? AlertTriangle : CheckCircle2;
  const resultKind = warning
    ? outcome.kind === "imported"
      ? "importWarning"
      : "warning"
    : outcome.kind;
  const titleKey = `preferences.backup.completion.${resultKind}.title`;
  const descriptionKey = `preferences.backup.completion.${resultKind}.description`;
  const label =
    outcome.kind === "renamed" ||
    outcome.kind === "restored" ||
    outcome.kind === "deleted"
      ? outcome.label
      : undefined;

  return (
    <div
      role={warning ? "alert" : "status"}
      aria-label={t(titleKey)}
      aria-live={warning ? "assertive" : "polite"}
      className={`flex flex-col gap-3 rounded-lg border p-4 sm:flex-row sm:items-center sm:justify-between ${
        warning
          ? "border-warning/30 bg-warning/5"
          : "border-success/30 bg-success/5"
      }`}
    >
      <div className="flex min-w-0 items-start gap-3">
        <span
          className={`flex h-9 w-9 shrink-0 items-center justify-center rounded-full ${
            warning
              ? "bg-warning/10 text-warning"
              : "bg-success/10 text-success"
          }`}
        >
          <Icon className="h-5 w-5" aria-hidden="true" />
        </span>
        <div className="min-w-0">
          <p className="text-body font-medium text-content">{t(titleKey)}</p>
          <p className="mt-0.5 text-caption leading-5 text-content-muted">
            {t(descriptionKey, { name: label })}
          </p>
        </div>
      </div>

      {warning && onReviewServices ? (
        <Button
          variant="secondary"
          size="sm"
          className="self-start sm:self-auto"
          onClick={onReviewServices}
        >
          {t("preferences.backup.completion.reviewServices")}
          <MoveRight className="h-4 w-4" aria-hidden="true" />
        </Button>
      ) : null}
    </div>
  );
}
