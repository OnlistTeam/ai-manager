import { CalendarDays, FileText, HardDrive } from "lucide-react";
import { useTranslation } from "react-i18next";
import type { OpenClawWorkspaceOverview } from "@/entities/openclaw-workspace";
import { MetricStrip } from "@/shared/ui/MetricStrip";
import { formatWorkspaceBytes } from "./workspaceFormat";

interface WorkspaceOverviewStripProps {
  overview: OpenClawWorkspaceOverview;
}

export function WorkspaceOverviewStrip({
  overview,
}: WorkspaceOverviewStripProps) {
  const { t, i18n } = useTranslation();
  return (
    <MetricStrip
      title={t("openClawWorkspace.overview.title")}
      description={t("openClawWorkspace.overview.description")}
      metrics={[
        {
          id: "files",
          label: t("openClawWorkspace.overview.files"),
          value: t("openClawWorkspace.overview.filesValue", {
            ready: overview.existingFiles,
            total: overview.files.length,
          }),
          icon: FileText,
        },
        {
          id: "memories",
          label: t("openClawWorkspace.overview.memories"),
          value: overview.dailyMemoryCount,
          icon: CalendarDays,
        },
        {
          id: "storage",
          label: t("openClawWorkspace.overview.storage"),
          value: formatWorkspaceBytes(overview.totalBytes, i18n.language),
          icon: HardDrive,
        },
      ]}
      footer={
        overview.limited ? (
          <p className="text-caption text-warning">
            {t("openClawWorkspace.overview.limited")}
          </p>
        ) : undefined
      }
    />
  );
}
