import { Play } from "lucide-react";
import { useTranslation } from "react-i18next";
import {
  useAvailableTerminals,
  useProductSettings,
  useSaveProductSettings,
  type TerminalAppId,
} from "@/entities/settings";
import { toErrorCopy } from "@/shared/lib/nativeError";
import { Button } from "@/shared/ui/Button";
import { CopyButton } from "@/shared/ui/CopyButton";
import { Modal } from "@/shared/ui/Modal";

interface SessionResumeModalProps {
  open: boolean;
  /** The manual resume command; `null` when the backend couldn't parse one, leaving the modal with only the "open directly" path. */
  command: string | null;
  resuming: boolean;
  resumed: boolean;
  error: Error | null;
  onOpenChange: (open: boolean) => void;
  onResume: () => void;
}

export function SessionResumeModal(props: SessionResumeModalProps) {
  const { t } = useTranslation();
  const terminals = useAvailableTerminals();
  const settings = useProductSettings();
  const saveSettings = useSaveProductSettings();
  const error = props.error ? toErrorCopy(props.error) : null;

  // When only the system terminal is installed, there's nothing to choose from, so this row shouldn't appear.
  const choices = terminals.data ?? [];
  const remembered = settings.data?.terminalApp ?? null;
  // The remembered terminal may have been uninstalled. In that case the
  // controlled <select> would silently sit on the first item; falling back
  // explicitly to the system terminal keeps the UI showing what will
  // actually happen next.
  const chosen: TerminalAppId =
    remembered !== null && choices.includes(remembered) ? remembered : "system";
  const saveError = saveSettings.error ? toErrorCopy(saveSettings.error) : null;

  return (
    <Modal
      open={props.open}
      onOpenChange={props.onOpenChange}
      title={t("sessions.resume.title")}
      size="md"
      dismissible={!props.resuming}
    >
      <div className="space-y-5">
        <div className="space-y-2">
          {choices.length > 1 ? (
            <>
              <select
                value={chosen}
                disabled={props.resuming || saveSettings.isPending}
                aria-label={t("sessions.resume.terminal.label")}
                onChange={(event) =>
                  saveSettings.mutate({
                    terminalApp: event.target.value as TerminalAppId,
                  })
                }
                className="w-full disabled:opacity-50"
              >
                {choices.map((id) => (
                  <option key={id} value={id}>
                    {t(`sessions.resume.terminal.${id}`)}
                  </option>
                ))}
              </select>
              {saveError ? (
                <p role="alert" className="text-caption text-danger">
                  {t(saveError.messageKey)}
                </p>
              ) : null}
            </>
          ) : null}
          <Button
            className="w-full"
            loading={props.resuming}
            onClick={props.onResume}
          >
            <Play className="h-4 w-4" aria-hidden="true" />
            {t("sessions.resume.launch")}
          </Button>
          {props.resumed ? (
            <p role="status" className="text-caption text-success">
              {t("sessions.resume.success")}
            </p>
          ) : null}
          {error ? (
            <p role="alert" className="text-caption text-danger">
              {t("sessions.resume.error")} {t(error.messageKey)}
            </p>
          ) : null}
        </div>

        {props.command ? (
          <div className="space-y-2 border-t border-hairline pt-4">
            <p className="text-caption text-content-muted">
              {t("sessions.resume.manualHint")}
            </p>
            <div className="flex items-start gap-2">
              <code className="min-w-0 flex-1 break-all rounded-lg border border-hairline bg-layer-1/30 px-2.5 py-2 text-mono-sm text-content-muted">
                {props.command}
              </code>
              <CopyButton
                value={props.command}
                label={t("sessions.resume.manualLabel")}
              />
            </div>
          </div>
        ) : null}
      </div>
    </Modal>
  );
}
