import { useTranslation } from "react-i18next";
import type { ErrorCopy } from "@/shared/lib/nativeError";

export interface ErrorDetailsDisclosureProps {
  copy: ErrorCopy;
}

/**
 * Spec §42's View Details, folded inside an inline error panel. The remediation
 * `retryOrViewDetails` points here, so a panel that shows it must also offer
 * this. `technicalMessage` arrives already redacted and truncated by the
 * backend (`platform::redact`) and is never the primary copy.
 */
export function ErrorDetailsDisclosure({ copy }: ErrorDetailsDisclosureProps) {
  const { t } = useTranslation();

  return (
    <details className="mt-1.5 text-caption">
      <summary className="cursor-pointer text-content-muted">
        {t("error.details.show")}
      </summary>
      <dl className="mt-1.5 grid grid-cols-[auto_1fr] gap-x-3 gap-y-1">
        <dt className="text-content-muted">{t("error.details.code")}</dt>
        <dd className="font-mono text-mono-sm text-content">{copy.code}</dd>
      </dl>
      {copy.technicalMessage ? (
        <pre className="mt-1.5 max-h-40 overflow-auto whitespace-pre-wrap break-all rounded-md bg-layer-1 p-2 font-mono text-mono-sm text-content">
          {copy.technicalMessage}
        </pre>
      ) : null}
    </details>
  );
}
