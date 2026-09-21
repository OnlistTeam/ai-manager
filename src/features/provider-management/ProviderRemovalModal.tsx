import { AlertCircle, AlertTriangle } from "lucide-react";
import { useRef } from "react";
import { useTranslation } from "react-i18next";
import type { Provider } from "@/entities/provider";
import { toErrorCopy } from "@/shared/lib/nativeError";
import { Button } from "@/shared/ui/Button";
import { Modal } from "@/shared/ui/Modal";
import { ServiceActionsPausedNotice } from "./ServiceActionsPausedNotice";

const IMPACT_KEYS = [
  "services.remove.point.local",
  "services.remove.point.account",
  "services.remove.point.otherTools",
] as const;

export interface ProviderRemovalModalProps {
  provider: Provider | null;
  toolName: string;
  busy: boolean;
  error: Error | null;
  mutationsBlocked?: boolean;
  onOpenChange: (open: boolean) => void;
  onConfirm: () => void;
}

export function ProviderRemovalModal({
  provider,
  toolName,
  busy,
  error,
  mutationsBlocked = false,
  onOpenChange,
  onConfirm,
}: ProviderRemovalModalProps) {
  const { t } = useTranslation();
  const cancelRef = useRef<HTMLButtonElement>(null);
  const copy = error ? toErrorCopy(error) : null;
  const providerName = provider?.name ?? "";
  const errorTitle = t("services.remove.errorTitle");

  return (
    <Modal
      open={provider !== null}
      size="md"
      dismissible={!busy}
      initialFocusRef={cancelRef}
      onOpenChange={onOpenChange}
      title={t("services.remove.title", { name: providerName })}
      description={t("services.remove.description", { tool: toolName })}
      footer={
        <>
          <Button
            ref={cancelRef}
            variant="secondary"
            disabled={busy}
            onClick={() => onOpenChange(false)}
          >
            {t("ds.action.cancel")}
          </Button>
          <Button
            variant="danger"
            disabled={mutationsBlocked}
            loading={busy}
            aria-label={
              copy
                ? t("services.remove.retryNamed", { name: providerName })
                : undefined
            }
            onClick={() => {
              if (!mutationsBlocked) onConfirm();
            }}
          >
            {t(copy ? "ds.action.retry" : "services.remove.confirm")}
          </Button>
        </>
      }
    >
      {mutationsBlocked ? <ServiceActionsPausedNotice /> : null}

      <ul className="flex flex-col gap-2 rounded-md bg-danger/10 p-3">
        {IMPACT_KEYS.map((key) => (
          <li
            key={key}
            className="flex items-start gap-2 text-caption text-content"
          >
            <AlertTriangle
              className="mt-0.5 h-4 w-4 shrink-0 text-danger"
              aria-hidden="true"
            />
            {t(key, { tool: toolName })}
          </li>
        ))}
      </ul>

      {copy ? (
        <div
          role="alert"
          aria-label={errorTitle}
          className="mt-3 flex items-start gap-2 rounded-md border border-danger/30 bg-danger/5 p-3"
        >
          <AlertCircle
            className="mt-0.5 h-4 w-4 shrink-0 text-danger"
            aria-hidden="true"
          />
          <div>
            <p className="text-caption font-medium text-content">
              {errorTitle}
            </p>
            <p className="mt-1.5 text-caption leading-5 text-content">
              {t(copy.messageKey)}
            </p>
            {copy.remediationKey ? (
              <p className="mt-0.5 text-caption leading-5 text-content-muted">
                {t(copy.remediationKey)}
              </p>
            ) : null}
            <p className="mt-0.5 text-caption leading-5 text-content-muted">
              {t("services.remove.errorRetry")}
            </p>
          </div>
        </div>
      ) : null}
    </Modal>
  );
}
