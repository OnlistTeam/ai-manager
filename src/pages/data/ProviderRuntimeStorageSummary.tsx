import { FolderClock, HardDrive } from "lucide-react";
import { useTranslation } from "react-i18next";
import type { ProviderRuntimeStorage } from "@/entities/provider";

interface ProviderRuntimeStorageSummaryProps {
  storage: ProviderRuntimeStorage;
}

export function ProviderRuntimeStorageSummary({
  storage,
}: ProviderRuntimeStorageSummaryProps) {
  const { t, i18n } = useTranslation();
  const locale = i18n.resolvedLanguage || i18n.language || "en";
  const lowerBound = storage.measurementLimited;

  const metrics = [
    {
      id: "total",
      icon: HardDrive,
      label: t("services.runtime.storage.total"),
      value: formatStorageBytes(storage.totalBytes, locale, lowerBound),
    },
    {
      id: "sessions",
      icon: FolderClock,
      label: t("services.runtime.storage.conversationBytes"),
      value: formatStorageBytes(storage.sessionBytes, locale, lowerBound),
    },
  ] as const;

  return (
    <div className="min-w-0">
      <dl className="flex flex-wrap gap-x-5 gap-y-2">
        {metrics.map(({ id, icon: Icon, label, value }) => (
          <div key={id} className="flex min-w-0 flex-wrap items-center gap-2">
            <dt className="flex min-w-0 items-center gap-1.5 text-caption text-content-muted">
              <Icon
                className="h-3.5 w-3.5 shrink-0 text-brand"
                aria-hidden="true"
              />
              <span className="min-w-0 break-words">{label}</span>
            </dt>
            <dd className="break-words text-body font-medium text-content">
              {value}
            </dd>
          </div>
        ))}
      </dl>
      {lowerBound ? (
        <p className="mt-2 text-[11px] leading-4 text-content-muted">
          {t("services.runtime.storage.limited")}
        </p>
      ) : null}
    </div>
  );
}

export function formatStorageBytes(
  bytes: number,
  locale: string,
  lowerBound = false,
): string {
  const units = ["B", "KB", "MB", "GB", "TB"] as const;
  let value = bytes;
  let unitIndex = 0;
  while (value >= 1024 && unitIndex < units.length - 1) {
    value /= 1024;
    unitIndex += 1;
  }
  const formatted = new Intl.NumberFormat(locale, {
    maximumFractionDigits: unitIndex === 0 ? 0 : 1,
    minimumFractionDigits: 0,
  }).format(value);
  return `${lowerBound ? "≥ " : ""}${formatted} ${units[unitIndex]}`;
}
