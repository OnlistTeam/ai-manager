import { LoaderCircle } from "lucide-react";
import { useEffect, useState } from "react";
import { useTranslation } from "react-i18next";
import { Card } from "@/shared/ui/Card";

/**
 * Shown while there is nothing to look at yet, which is when a first import
 * can run for minutes. Elapsed time is not a fake progress percentage;
 * parsing stays native.
 */
export function UsageSyncStatus({ startedAt }: { startedAt: number }) {
  const { t } = useTranslation();
  const [now, setNow] = useState(Date.now);
  useEffect(() => {
    const timer = window.setInterval(() => setNow(Date.now()), 1000);
    return () => window.clearInterval(timer);
  }, []);
  const seconds = Math.max(0, Math.floor((now - startedAt) / 1000));
  return (
    <Card
      role="status"
      aria-label={t("usage.sync.running")}
      className="flex items-start gap-3"
    >
      <LoaderCircle
        className="mt-0.5 h-4 w-4 shrink-0 text-brand motion-safe:animate-spin"
        aria-hidden="true"
      />
      <div>
        <p className="text-body text-content">{t("usage.sync.running")}</p>
        <p className="mt-1 text-caption text-content-muted">
          {t(
            seconds >= 10
              ? "usage.sync.largeHistory"
              : "usage.sync.runningHint",
          )}
        </p>
        <p aria-live="off" className="mt-1 text-caption text-content-muted">
          {t("usage.sync.elapsed", { seconds })}
        </p>
      </div>
    </Card>
  );
}

/** With a summary already on screen, a sync only needs a word beside the title. */
export function UsageSyncIndicator() {
  const { t } = useTranslation();
  return (
    <span
      role="status"
      aria-label={t("usage.sync.running")}
      className="flex items-center gap-1.5 text-caption text-content-muted"
    >
      <LoaderCircle
        className="h-3.5 w-3.5 shrink-0 text-brand motion-safe:animate-spin"
        aria-hidden="true"
      />
      {t("usage.sync.running")}
    </span>
  );
}
