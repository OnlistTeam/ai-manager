import { CheckCircle2, Waypoints } from "lucide-react";
import { useTranslation } from "react-i18next";
import type { ImportSummary as ImportSummaryValue } from "@/entities/import";
import { ImportSummary } from "./ImportSummary";

export interface ImportCompletionProps {
  summary: ImportSummaryValue;
  showHeading?: boolean;
}

/** A durable completion state, not a toast that disappears before the next step is understood. */
export function ImportCompletion({
  summary,
  showHeading = true,
}: ImportCompletionProps) {
  const { t } = useTranslation();

  return (
    <div role="status" aria-live="polite" className="flex flex-col gap-4">
      {showHeading ? (
        <div className="flex items-start gap-3">
          <span className="flex h-9 w-9 shrink-0 items-center justify-center rounded-full bg-success/10 text-success">
            <CheckCircle2 className="h-5 w-5" aria-hidden="true" />
          </span>
          <div>
            <p className="text-body font-medium text-content">
              {t("preferences.import.completion.title")}
            </p>
            <p className="mt-0.5 text-caption leading-5 text-content-muted">
              {t("preferences.import.completion.description")}
            </p>
          </div>
        </div>
      ) : null}

      <ImportSummary
        summary={summary}
        label={t("preferences.import.completion.countLabel")}
      />

      <div className="flex items-start gap-2 rounded-lg bg-layer-1 p-3">
        <Waypoints
          className="mt-0.5 h-4 w-4 shrink-0 text-brand"
          aria-hidden="true"
        />
        <div>
          <p className="text-caption font-medium text-content">
            {t("preferences.import.completion.nextStepTitle")}
          </p>
          <p className="mt-0.5 text-caption leading-5 text-content-muted">
            {t("preferences.import.completion.nextStep")}
          </p>
        </div>
      </div>
    </div>
  );
}
