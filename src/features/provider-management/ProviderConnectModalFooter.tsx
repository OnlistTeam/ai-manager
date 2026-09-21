import { useTranslation } from "react-i18next";
import { Button } from "@/shared/ui/Button";

interface ProviderConnectModalFooterProps {
  formId: string;
  busy: boolean;
  mutationsBlocked: boolean;
  error: Error | null;
  onCancel: () => void;
}

/** Cancel / Connect (or Retry) pair shared by the modal's own `footer` slot. */
export function ProviderConnectModalFooter({
  formId,
  busy,
  mutationsBlocked,
  error,
  onCancel,
}: ProviderConnectModalFooterProps) {
  const { t } = useTranslation();
  return (
    <>
      <Button variant="ghost" disabled={busy} onClick={onCancel}>
        {t("ds.action.cancel")}
      </Button>
      <Button
        type="submit"
        form={formId}
        disabled={mutationsBlocked}
        loading={busy}
        aria-label={error ? t("services.connect.retryConnect") : undefined}
      >
        {t(error ? "ds.action.retry" : "ds.action.connect")}
      </Button>
    </>
  );
}
