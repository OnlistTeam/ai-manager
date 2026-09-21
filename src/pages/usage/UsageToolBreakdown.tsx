import { Layers3 } from "lucide-react";
import { useTranslation } from "react-i18next";
import type { UsageToolBreakdown as Breakdown } from "@/entities/usage";
import { Card } from "@/shared/ui/Card";
import { ToolArtwork } from "@/shared/ui/ToolArtwork";
import { formatCount, formatUsd } from "./usageFormat";

export function UsageToolBreakdown({ items }: { items: readonly Breakdown[] }) {
  const { t, i18n } = useTranslation();
  const locale = i18n.resolvedLanguage ?? i18n.language;
  const maximum = Math.max(1, ...items.map((item) => item.metrics.tokens));

  return (
    <section
      aria-labelledby="usage-tools-title"
      className="flex min-w-0 flex-col gap-3"
    >
      <div>
        <p className="flex items-center gap-2 text-caption font-medium text-brand">
          <Layers3 className="h-4 w-4" aria-hidden="true" />
          {t("usage.tools.eyebrow")}
        </p>
        <h2 id="usage-tools-title" className="mt-1 text-heading text-content">
          {t("usage.tools.title")}
        </h2>
      </div>

      <ul className="grid gap-3 md:grid-cols-2">
        {items.map((item) => {
          const share = Math.max(3, (item.metrics.tokens / maximum) * 100);
          return (
            <li key={item.tool}>
              <Card
                padding="md"
                className="h-full min-w-0 rounded-xl shadow-sm"
              >
                <div className="flex min-w-0 items-center gap-3">
                  <ToolArtwork toolId={item.tool} className="h-11 w-11" />
                  <div className="min-w-0 flex-1">
                    <h3 className="text-body font-semibold text-content">
                      {t(`usage.tool.${item.tool}`)}
                    </h3>
                    <p className="mt-0.5 text-caption text-content-muted">
                      {t("usage.tools.requests", {
                        value: formatCount(item.metrics.requests, locale),
                      })}
                    </p>
                  </div>
                  <p className="shrink-0 text-body font-semibold tabular-nums text-content">
                    {formatUsd(item.metrics.estimatedCostUsd, locale)}
                  </p>
                </div>
                <div className="mt-4 h-1.5 overflow-hidden rounded-full bg-layer-1">
                  <div
                    className="h-full rounded-full bg-brand/75"
                    style={{ width: `${share}%` }}
                  />
                </div>
                <p className="mt-2 text-caption tabular-nums text-content-muted">
                  {t("usage.tools.tokens", {
                    value: formatCount(item.metrics.tokens, locale),
                  })}
                </p>
              </Card>
            </li>
          );
        })}
      </ul>
    </section>
  );
}
