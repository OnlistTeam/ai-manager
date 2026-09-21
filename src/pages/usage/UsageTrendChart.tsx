import { BarChart3 } from "lucide-react";
import { useId } from "react";
import { useTranslation } from "react-i18next";
import type { UsageDay } from "@/entities/usage";
import { Card } from "@/shared/ui/Card";
import { formatCount, formatUsageDate } from "./usageFormat";

export function UsageTrendChart({ days }: { days: readonly UsageDay[] }) {
  const { t, i18n } = useTranslation();
  const headingId = useId();
  const locale = i18n.resolvedLanguage ?? i18n.language;
  const maximum = Math.max(1, ...days.map((day) => day.tokens));
  const total = days.reduce((sum, day) => sum + day.tokens, 0);
  const first = days.at(0);
  const last = days.at(-1);

  return (
    <Card padding="lg" className="min-w-0 rounded-xl shadow-md">
      <div className="flex items-start justify-between gap-4">
        <div className="min-w-0">
          <p className="flex items-center gap-2 text-caption font-medium text-brand">
            <BarChart3 className="h-4 w-4" aria-hidden="true" />
            {t("usage.trend.eyebrow")}
          </p>
          <h2 id={headingId} className="mt-1 text-heading text-content">
            {t("usage.trend.title")}
          </h2>
          <p className="mt-1 text-caption text-content-muted">
            {t("usage.trend.total", { value: formatCount(total, locale) })}
          </p>
        </div>
      </div>

      <div
        role="img"
        aria-labelledby={headingId}
        aria-label={t("usage.trend.label", {
          value: formatCount(total, locale),
        })}
        className="mt-6"
      >
        <div
          aria-hidden="true"
          className="flex h-40 items-end gap-1 rounded-xl border border-hairline bg-layer-1 px-3 pb-3 pt-5"
        >
          {days.map((day) => {
            const height =
              day.tokens === 0 ? 2 : Math.max(6, (day.tokens / maximum) * 100);
            return (
              <span
                key={day.date}
                title={`${day.date}: ${formatCount(day.tokens, locale)}`}
                className="min-w-0 flex-1 rounded-t-sm bg-brand/75 transition-[height] duration-base ease-standard"
                style={{ height: `${height}%` }}
              />
            );
          })}
        </div>
        <div className="mt-2 flex justify-between gap-3 text-caption text-content-muted">
          <span>{first ? formatUsageDate(first.date, locale) : ""}</span>
          <span>{last ? formatUsageDate(last.date, locale) : ""}</span>
        </div>
      </div>
    </Card>
  );
}
