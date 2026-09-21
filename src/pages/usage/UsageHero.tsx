import { Activity, Coins, Database, Gauge, Sparkles } from "lucide-react";
import { useTranslation } from "react-i18next";
import type { UsageOverview } from "@/entities/usage";
import { MetricStrip } from "@/shared/ui/MetricStrip";
import { formatCount, formatPercent, formatUsd } from "./usageFormat";

const METRICS = [
  { key: "tokens", icon: Database, tone: "text-brand" },
  { key: "requests", icon: Activity, tone: "text-success" },
  { key: "success", icon: Gauge, tone: "text-warning" },
  { key: "cache", icon: Sparkles, tone: "text-brand" },
] as const;

export function UsageHero({ overview }: { overview: UsageOverview }) {
  const { t, i18n } = useTranslation();
  const locale = i18n.resolvedLanguage ?? i18n.language;
  const values = {
    tokens: formatCount(overview.summary.tokens, locale),
    requests: formatCount(overview.summary.requests, locale),
    success: formatPercent(overview.summary.successRatePercent, locale),
    cache: formatPercent(overview.summary.cacheHitRatePercent, locale),
  };

  return (
    <MetricStrip
      eyebrow={t("usage.hero.eyebrow", { days: overview.periodDays })}
      eyebrowIcon={Coins}
      title={t("usage.hero.cost")}
      highlight={formatUsd(overview.summary.estimatedCostUsd, locale)}
      description={t("usage.hero.estimate")}
      metrics={METRICS.map(({ key, icon, tone }) => ({
        id: key,
        label: t(`usage.hero.metric.${key}`),
        value: values[key],
        icon,
        iconClassName: tone,
      }))}
    />
  );
}
