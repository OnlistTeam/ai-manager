import { useRef, type ReactNode, type RefObject } from "react";
import {
  AlertTriangle,
  FolderOpen,
  RefreshCw,
  ShieldCheck,
  Shuffle,
} from "lucide-react";
import { useTranslation } from "react-i18next";
import type { Tool } from "@/entities/tool";
import { Button } from "@/shared/ui/Button";
import { Modal } from "@/shared/ui/Modal";
import { ToolActionError } from "./ToolActionError";

export interface OpenToolModalProps {
  tool: Tool | null;
  busy?: boolean;
  confirmDisabled?: boolean;
  error: Error | null;
  notice?: ReactNode;
  providerRecovery?: {
    providerName: string;
    canTryNext: boolean;
    trying: boolean;
    unavailable: boolean;
    onTryNext: () => void;
  } | null;
  returnFocusFallbackRef?: RefObject<HTMLElement>;
  onOpenChange: (open: boolean) => void;
  onLaunchDefault: () => void;
  onChooseFolder: () => void;
}

export function OpenToolModal({
  tool,
  busy = false,
  confirmDisabled = false,
  error,
  notice,
  providerRecovery,
  returnFocusFallbackRef,
  onOpenChange,
  onLaunchDefault,
  onChooseFolder,
}: OpenToolModalProps) {
  const { t } = useTranslation();
  const confirmRef = useRef<HTMLButtonElement>(null);
  const name = tool?.name ?? "";
  const retrying = error !== null || providerRecovery != null;

  return (
    <Modal
      open={tool !== null}
      onOpenChange={onOpenChange}
      dismissible={!busy}
      initialFocusRef={confirmRef}
      returnFocusFallbackRef={returnFocusFallbackRef}
      size="sm"
      title={t("tools.open.title", { name })}
      description={t("tools.open.description", { name })}
      footer={
        <>
          <Button
            variant="secondary"
            disabled={busy}
            onClick={() => onOpenChange(false)}
          >
            {t("ds.action.cancel")}
          </Button>
          <Button
            variant="secondary"
            disabled={busy || confirmDisabled}
            onClick={onChooseFolder}
          >
            <FolderOpen className="h-4 w-4" aria-hidden="true" />
            {t("tools.open.chooseFolder")}
          </Button>
          <Button
            ref={confirmRef}
            disabled={confirmDisabled}
            loading={busy}
            aria-label={
              retrying ? t("tools.open.retryNamed", { name }) : undefined
            }
            onClick={onLaunchDefault}
          >
            {busy ? null : retrying ? (
              <RefreshCw className="h-4 w-4" aria-hidden="true" />
            ) : (
              <FolderOpen className="h-4 w-4" aria-hidden="true" />
            )}
            {t(retrying ? "tools.open.retry" : "tools.open.launchDefault")}
          </Button>
        </>
      }
    >
      <div className="flex items-start gap-3 rounded-lg border border-success/20 bg-success/10 p-3">
        <ShieldCheck
          className="mt-0.5 h-5 w-5 shrink-0 text-success"
          aria-hidden="true"
        />
        <p className="text-caption text-content-muted">
          <span className="mb-0.5 block text-body text-content">
            {t("tools.open.localTitle")}
          </span>
          {t("tools.open.localHint")}
        </p>
      </div>
      {providerRecovery ? (
        <div className="mt-3 rounded-xl border border-warning/20 bg-warning/[0.07] p-3">
          <div className="flex items-start gap-3">
            <AlertTriangle
              className="mt-0.5 h-5 w-5 shrink-0 text-warning"
              aria-hidden="true"
            />
            <div className="min-w-0">
              <p className="text-body font-medium text-content">
                {t("tools.open.providerUnreachable", {
                  name: providerRecovery.providerName,
                })}
              </p>
              <p className="mt-0.5 text-caption leading-5 text-content-muted">
                {t("tools.open.providerUnreachableHint")}
              </p>
            </div>
          </div>
          {providerRecovery.canTryNext ? (
            <Button
              className="mt-3"
              variant="secondary"
              size="sm"
              loading={providerRecovery.trying}
              disabled={busy}
              onClick={providerRecovery.onTryNext}
            >
              {providerRecovery.trying ? null : (
                <Shuffle className="h-4 w-4" aria-hidden="true" />
              )}
              {t("services.failover.tryNext")}
            </Button>
          ) : (
            <p role="status" className="mt-2 text-caption text-content-muted">
              {t("tools.open.noSavedAlternative")}
            </p>
          )}
          {providerRecovery.unavailable ? (
            <p role="status" className="mt-2 text-caption text-content-muted">
              {t("services.failover.noneAvailable")}
            </p>
          ) : null}
        </div>
      ) : null}
      {notice}
      {error ? <ToolActionError error={error} className="mt-3" /> : null}
    </Modal>
  );
}
