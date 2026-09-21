import { Check, ShieldCheck } from "lucide-react";
import { useTranslation } from "react-i18next";

/**
 * The guarantees that make an overwrite-capable import understandable.
 *
 * `skipHarmless` is the one people actually need. The dialog appears at first
 * launch and offers to import "configuration", which readers take to mean the
 * configuration of their own tools; they then assume declining leaves the
 * product unable to manage those tools. It does not. Every tool's own
 * configuration is read at every startup regardless, and this import only ever
 * concerns a different product's database.
 */
export function ImportSafetyNotice() {
  const { t } = useTranslation();
  const points = [
    "preferences.import.safety.skipHarmless",
    "preferences.import.safety.sourceUntouched",
    "preferences.import.safety.backupFirst",
    "preferences.import.collision",
  ] as const;

  return (
    <section
      aria-label={t("preferences.import.safety.title")}
      className="rounded-lg border border-brand/15 bg-brand/5 p-3"
    >
      <div className="flex items-center gap-2 text-body font-medium text-content">
        <ShieldCheck className="h-4 w-4 text-brand" aria-hidden="true" />
        {t("preferences.import.safety.title")}
      </div>
      <ul className="mt-2 flex flex-col gap-1.5">
        {points.map((key) => (
          <li
            key={key}
            className="flex items-start gap-2 text-caption leading-5 text-content"
          >
            <Check
              className="mt-0.5 h-3.5 w-3.5 shrink-0 text-success"
              aria-hidden="true"
            />
            {t(key)}
          </li>
        ))}
      </ul>
    </section>
  );
}
