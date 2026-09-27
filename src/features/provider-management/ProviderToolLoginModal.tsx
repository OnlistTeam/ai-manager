import { useTranslation } from "react-i18next";
import type { ToolLoginAccount } from "@/entities/provider";
import { Button } from "@/shared/ui/Button";
import { Modal } from "@/shared/ui/Modal";
import { ProviderConnectFailure } from "./ProviderConnectFailure";
import { ServiceActionsPausedNotice } from "./ServiceActionsPausedNotice";

export interface ProviderToolLoginModalProps {
  /** `null` keeps the dialog closed. */
  account: ToolLoginAccount | null;
  toolName: string;
  /** The entry is already in the list, so there is nothing to add. */
  saved: boolean;
  busy?: boolean;
  error?: Error | null;
  mutationsBlocked?: boolean;
  onOpenChange: (open: boolean) => void;
  onRestore: () => void;
  onShowList: () => void;
}

/**
 * The subscription card's dialog (ADR-0057). It adds one endpoint, the one
 * that leaves the tool on its own sign-in, and says so plainly: signing in
 * happens inside the tool, and AI Manager never sees the account.
 */
export function ProviderToolLoginModal({
  account,
  toolName,
  saved,
  busy = false,
  error = null,
  mutationsBlocked = false,
  onOpenChange,
  onRestore,
  onShowList,
}: ProviderToolLoginModalProps) {
  const { t } = useTranslation();
  if (account === null) return null;
  const title = t(`services.add.login.${account}`);

  return (
    <Modal
      open
      onOpenChange={onOpenChange}
      dismissible={!busy}
      title={title}
      description={t("services.login.description", { tool: toolName })}
      footer={
        saved ? (
          <Button onClick={onShowList}>{t("services.login.showList")}</Button>
        ) : (
          <>
            <Button
              variant="ghost"
              disabled={busy}
              onClick={() => onOpenChange(false)}
            >
              {t("ds.action.cancel")}
            </Button>
            <Button
              disabled={mutationsBlocked}
              loading={busy}
              onClick={onRestore}
            >
              {t("services.login.add")}
            </Button>
          </>
        )
      }
    >
      {mutationsBlocked ? <ServiceActionsPausedNotice /> : null}
      <div className="flex flex-col gap-3">
        {error ? <ProviderConnectFailure error={error} /> : null}
        <p className="text-body text-content">
          {t("services.login.signInThere", { tool: toolName })}
        </p>
        <p className="text-caption text-content-muted">
          {t(`services.login.how.${account}`)}
        </p>
        {saved ? (
          <p className="text-caption text-content-muted" role="status">
            {t("services.login.alreadySaved")}
          </p>
        ) : null}
      </div>
    </Modal>
  );
}
