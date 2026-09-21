import { Blocks, Cloud, Sparkles } from "lucide-react";
import { useTranslation } from "react-i18next";
import type { ImportSummary as ImportSummaryValue } from "@/entities/import";

export interface ImportSummaryProps {
  summary: ImportSummaryValue;
  label?: string;
}

export function ImportSummary({ summary, label }: ImportSummaryProps) {
  const { t } = useTranslation();
  const rows = [
    {
      key: "services",
      icon: Cloud,
      label: t("preferences.import.count.services", {
        count: summary.services,
      }),
    },
    {
      key: "mcp",
      icon: Blocks,
      label: t("preferences.import.count.mcpServers", {
        count: summary.mcpServers,
      }),
    },
    {
      key: "skills",
      icon: Sparkles,
      label: t("preferences.import.count.skills", { count: summary.skills }),
    },
  ];

  return (
    <ul
      aria-label={label ?? t("preferences.import.count.label")}
      className="grid gap-2 [grid-template-columns:repeat(auto-fit,minmax(9.25rem,1fr))]"
    >
      {rows.map(({ key, icon: Icon, label }) => (
        <li
          key={key}
          className="flex items-center gap-2 whitespace-nowrap rounded-md bg-layer-1 px-3 py-2 text-body text-content"
        >
          <Icon className="h-4 w-4 shrink-0 text-brand" aria-hidden="true" />
          {label}
        </li>
      ))}
    </ul>
  );
}
