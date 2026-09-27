import { AlertCircle } from "lucide-react";
import { useTranslation } from "react-i18next";

export interface McpFormAlertProps {
  messageKey: string;
  remediationKey?: string | null;
}

/** A failure the MCP form cannot fix field by field, in the form's own style. */
export function McpFormAlert({
  messageKey,
  remediationKey,
}: McpFormAlertProps) {
  const { t } = useTranslation();
  return (
    <div
      role="alert"
      className="flex items-start gap-2 rounded-md border border-danger/30 bg-danger/5 p-3"
    >
      <AlertCircle
        className="mt-0.5 h-4 w-4 shrink-0 text-danger"
        aria-hidden="true"
      />
      <div>
        <p className="text-caption font-medium text-content">{t(messageKey)}</p>
        {remediationKey ? (
          <p className="mt-0.5 text-caption text-content-muted">
            {t(remediationKey)}
          </p>
        ) : null}
      </div>
    </div>
  );
}
