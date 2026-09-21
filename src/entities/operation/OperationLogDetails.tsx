import { ChevronDown, Terminal } from "lucide-react";
import { useTranslation } from "react-i18next";
import type { OperationLogEntry } from "@/native";
import { cn } from "@/shared/ui/cn";

interface OperationLogDetailsProps {
  logs: OperationLogEntry[];
  className?: string;
}

function formatLogTime(timestamp: number, locale: string): string {
  const date = new Date(timestamp);
  if (Number.isNaN(date.getTime())) return "--:--:--";
  return new Intl.DateTimeFormat(locale, {
    hour: "2-digit",
    minute: "2-digit",
    second: "2-digit",
  }).format(date);
}

/** stderr is a transport stream, not a severity. Package managers routinely
 * send non-fatal warnings there, so classify the familiar warning prefixes
 * before choosing the user-facing label and colour. */
export function isDiagnosticWarning(text: string): boolean {
  const firstLine = text.split(/\r?\n/, 1)[0] ?? "";
  return /^\s*(?:\[[^\]]+\]\s*)?(?:(?:npm|pnpm|yarn)\s+)?warn(?:ing)?\b/i.test(
    firstLine,
  );
}

/**
 * Collapsed-by-default technical output shared by the active tool card and
 * Task Center. Native owns redaction, truncation and the in-memory lifetime;
 * this component only reveals the already-bounded stream on explicit demand.
 */
export function OperationLogDetails({
  logs,
  className,
}: OperationLogDetailsProps) {
  const { t, i18n } = useTranslation();
  if (logs.length === 0) return null;

  const locale = i18n.resolvedLanguage || i18n.language || "en";

  return (
    <details
      className={cn("group/log mt-2 border-t border-hairline pt-2", className)}
    >
      <summary
        aria-label={t("taskCenter.logs.toggle", { count: logs.length })}
        className="flex cursor-pointer list-none items-center gap-1.5 rounded-md py-1 text-caption font-medium text-content-muted transition-colors hover:text-content focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-brand/35 [&::-webkit-details-marker]:hidden"
      >
        <Terminal className="h-3.5 w-3.5" aria-hidden="true" />
        <span>{t("taskCenter.logs.title", { count: logs.length })}</span>
        <ChevronDown
          className="h-3.5 w-3.5 transition-transform group-open/log:rotate-180"
          aria-hidden="true"
        />
      </summary>

      <p className="mt-1.5 text-[11px] leading-4 text-content-muted">
        {t("taskCenter.logs.privacy")}
      </p>
      <ol
        data-selectable-text=""
        className="scrollbar-subtle mt-2 max-h-52 space-y-1 overflow-auto rounded-lg border border-hairline bg-layer-1/55 px-2 py-2 font-mono text-[11px] leading-4"
      >
        {logs.map((entry, index) => {
          const text = entry.messageKey
            ? t(entry.messageKey)
            : (entry.detail ?? "");
          const date = new Date(entry.timestamp);
          const dateTime = Number.isNaN(date.getTime())
            ? undefined
            : date.toISOString();
          const warning = entry.kind === "stderr" && isDiagnosticWarning(text);
          return (
            <li
              key={`${entry.timestamp}-${index}`}
              className="grid grid-cols-[4.5rem_3.75rem_minmax(0,1fr)] items-start gap-1.5"
            >
              <time dateTime={dateTime} className="text-content-faint">
                {formatLogTime(entry.timestamp, locale)}
              </time>
              <span
                className={cn(
                  "uppercase tracking-wide text-content-muted",
                  warning && "text-warning",
                  entry.kind === "stderr" && !warning && "text-content-subtle",
                )}
              >
                {t(
                  warning
                    ? "taskCenter.logs.kind.warning"
                    : `taskCenter.logs.kind.${entry.kind}`,
                )}
              </span>
              <span className="min-w-0 whitespace-pre-wrap break-words text-content-subtle">
                {text}
              </span>
            </li>
          );
        })}
      </ol>
    </details>
  );
}
