import { LoaderCircle } from "lucide-react";
import { useTranslation } from "react-i18next";
import { cn } from "./cn";

export interface DetectionStatusProps {
  label: string;
  className?: string;
}

/** A compact desktop status replaces web-like blocks of pulsing placeholders. */
export function DetectionStatus({ label, className }: DetectionStatusProps) {
  const { t } = useTranslation();

  return (
    <div
      role="status"
      aria-live="polite"
      aria-label={label}
      className={cn(
        "flex min-h-16 items-center gap-3 rounded-xl border border-hairline bg-layer-1 px-4 py-3 shadow-sm",
        className,
      )}
    >
      <span className="flex h-9 w-9 shrink-0 items-center justify-center rounded-full border border-brand/20 bg-brand/10 text-brand shadow-sm">
        <LoaderCircle
          className="h-5 w-5 motion-safe:animate-spin"
          aria-hidden="true"
        />
      </span>
      <span className="min-w-0">
        <span className="block text-body font-medium text-content">
          {t("common.detecting")}
        </span>
        <span className="block text-caption text-content-muted">{label}</span>
      </span>
    </div>
  );
}
