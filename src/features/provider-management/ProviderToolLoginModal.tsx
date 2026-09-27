import { Loader2 } from "lucide-react";
import { useTranslation } from "react-i18next";
import type {
  ProviderCreateResult,
  ToolId,
  ToolLoginAccount,
  ToolLoginStatus,
} from "@/entities/provider";
import { Button } from "@/shared/ui/Button";
import { CopyButton } from "@/shared/ui/CopyButton";
import { Modal } from "@/shared/ui/Modal";
import { ProviderConnectFailure } from "./ProviderConnectFailure";
import { ServiceActionsPausedNotice } from "./ServiceActionsPausedNotice";
import { toolLoginSummary } from "./toolLoginCopy";
import { useToolSignIn } from "./useToolSignIn";

/** Accounts kept side by side, each an endpoint of its own (ADR-0061). */
const SEVERAL_ACCOUNTS: readonly ToolLoginAccount[] = ["claude", "chatGpt"];

export interface ProviderToolLoginModalProps {
  /** `null` keeps the dialog closed. */
  account: ToolLoginAccount | null;
  tool: ToolId;
  toolName: string;
  /** The entry that follows the tool's own sign-in is in the list. */
  saved: boolean;
  /** What the tool says about its sign-in; unread or unknown shows nothing. */
  status?: ToolLoginStatus;
  busy?: boolean;
  error?: Error | null;
  mutationsBlocked?: boolean;
  onOpenChange: (open: boolean) => void;
  /** Puts back the entry that follows the tool's own sign-in. */
  onRestore: () => void;
  /** A sign-in finished: the endpoint it added or updated. */
  onSignedIn: (result: ProviderCreateResult) => void;
}

/**
 * The subscription card's dialog. It signs in from AI Manager: the vendor's
 * page opens in the browser and the dialog waits for it (ADR-0061). It also
 * says whether the tool is signed in right now (ADR-0060).
 */
export function ProviderToolLoginModal({
  account,
  ...props
}: ProviderToolLoginModalProps) {
  if (account === null) return null;
  return <SignInDialog account={account} {...props} />;
}

function SignInDialog({
  account,
  tool,
  toolName,
  saved,
  status,
  busy = false,
  error = null,
  mutationsBlocked = false,
  onOpenChange,
  onRestore,
  onSignedIn,
}: Omit<ProviderToolLoginModalProps, "account"> & {
  account: ToolLoginAccount;
}) {
  const { t } = useTranslation();
  const signIn = useToolSignIn(tool, onSignedIn);
  const summary = toolLoginSummary(status, t);
  const progress = signIn.progress;
  const waiting = signIn.waiting;
  const several = SEVERAL_ACCOUNTS.includes(account);
  const signedIn = status?.state === "signedIn";

  const close = (open: boolean) => {
    if (!open) signIn.cancel();
    onOpenChange(open);
  };

  const actionLabel =
    progress?.phase === "failed"
      ? t("services.login.tryAgain")
      : signedIn && several
        ? t("services.login.signInAnother")
        : t("services.login.signIn");

  return (
    <Modal
      open
      onOpenChange={close}
      dismissible={!busy}
      title={t(`services.add.login.${account}`)}
      description={t("services.login.description")}
      footer={
        <>
          <Button variant="ghost" disabled={busy} onClick={() => close(false)}>
            {t("ds.action.cancel")}
          </Button>
          {waiting ? null : (
            <Button
              disabled={mutationsBlocked || busy}
              loading={signIn.starting}
              onClick={signIn.begin}
            >
              {actionLabel}
            </Button>
          )}
        </>
      }
    >
      {mutationsBlocked ? <ServiceActionsPausedNotice /> : null}
      <div className="flex flex-col gap-3">
        {signIn.error ? <ProviderConnectFailure error={signIn.error} /> : null}
        {error ? <ProviderConnectFailure error={error} /> : null}
        {summary ? (
          <p className="text-body text-content" role="status">
            {t("services.login.current", { tool: toolName, status: summary })}
          </p>
        ) : null}
        {waiting ? (
          <div className="flex flex-col gap-2" aria-live="polite">
            <p className="flex items-center gap-2 text-body text-content">
              <Loader2
                className="h-4 w-4 animate-spin text-content-muted"
                aria-hidden
              />
              {t("services.login.waiting")}
            </p>
            {progress?.code ? (
              <div className="flex items-center gap-2">
                <span className="text-caption text-content-muted">
                  {t("services.login.code")}
                </span>
                <code className="rounded-md bg-layer-2 px-2 py-0.5 font-mono text-body text-content">
                  {progress.code}
                </code>
                <CopyButton
                  value={progress.code}
                  label={t("services.login.codeName")}
                />
              </div>
            ) : null}
            {progress?.url ? (
              <div className="flex items-center gap-1">
                <Button
                  variant="secondary"
                  size="xs"
                  onClick={signIn.openAgain}
                >
                  {t("services.login.openAgain")}
                </Button>
                <CopyButton
                  value={progress.url}
                  label={t("services.login.linkName")}
                />
              </div>
            ) : null}
          </div>
        ) : (
          <>
            {progress?.phase === "failed" && progress.failure ? (
              <p className="text-body text-warning" role="alert">
                {t(`services.login.failure.${progress.failure}`, {
                  tool: toolName,
                })}
              </p>
            ) : null}
            <p className="text-caption text-content-muted">
              {t(
                several
                  ? "services.login.browserHintSeveral"
                  : "services.login.browserHint",
              )}
            </p>
            {saved ? null : (
              <Button
                variant="ghost"
                size="xs"
                className="self-start"
                disabled={mutationsBlocked}
                loading={busy}
                onClick={onRestore}
              >
                {t("services.login.useToolLogin", { tool: toolName })}
              </Button>
            )}
          </>
        )}
      </div>
    </Modal>
  );
}
