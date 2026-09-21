import { FolderClock, HardDrive, Sparkles } from "lucide-react";
import { useTranslation } from "react-i18next";
import type { Tool } from "@/entities/tool";
import { MetricStrip } from "@/shared/ui/MetricStrip";
import { formatStorageBytes } from "./ProviderRuntimeStorageSummary";
import { useDataOverview } from "./useDataOverview";

/** The state of the installed-tools inventory itself; it determines whether there are any tools left to sum below. */
export type DataInventoryState = "pending" | "unavailable" | "ready";

/** No inventory means no total. The dash doesn't need translation; the footnote explains why. */
const UNAVAILABLE_VALUE = "—";

export interface DataOverviewStripProps {
  manageable: readonly Tool[];
  inventory: DataInventoryState;
}

/**
 * Cross-tool summary strip at the top of the page. It sits at the very top
 * because the local session area below opens in an empty state on a fresh
 * machine — the whole page shouldn't open empty, and this summary data is
 * available at zero cost thanks to startup warmup. The two metrics are the
 * same two cells left over from the single-tool panel
 * (`ProviderRuntimeStorageSummary`), just switched from "this one tool" to
 * "summed across every manageable tool".
 */
export function DataOverviewStrip({
  manageable,
  inventory,
}: DataOverviewStripProps) {
  const { t, i18n } = useTranslation();
  const overview = useDataOverview(manageable);
  const locale = i18n.resolvedLanguage || i18n.language || "en";
  const measuring = inventory === "pending" || overview.pending;

  const value = (bytes: number): string =>
    inventory === "unavailable"
      ? UNAVAILABLE_VALUE
      : measuring
        ? t("data.overview.measuring")
        : formatStorageBytes(bytes, locale, overview.lowerBound);

  const footer =
    inventory === "unavailable"
      ? t("data.overview.unavailable")
      : measuring
        ? null
        : overview.failedTools.length > 0
          ? t("data.overview.partial")
          : overview.lowerBound
            ? t("data.overview.limited")
            : null;

  return (
    <MetricStrip
      eyebrow={t("data.overview.eyebrow")}
      eyebrowIcon={Sparkles}
      title={t("data.overview.title")}
      description={t("data.overview.description")}
      metrics={[
        {
          id: "totalBytes",
          label: t("data.overview.metric.totalBytes"),
          value: value(overview.totalBytes),
          icon: HardDrive,
        },
        {
          id: "conversationBytes",
          label: t("data.overview.metric.conversationBytes"),
          value: value(overview.conversationBytes),
          icon: FolderClock,
        },
      ]}
      footer={
        footer ? (
          <p className="text-caption text-warning">{footer}</p>
        ) : undefined
      }
    />
  );
}
