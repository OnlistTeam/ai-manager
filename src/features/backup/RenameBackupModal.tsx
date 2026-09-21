import { useEffect, useRef, useState } from "react";
import { AlertCircle } from "lucide-react";
import { useTranslation } from "react-i18next";
import type { BackupFile } from "@/entities/backup";
import type { ErrorCopy } from "@/shared/lib/nativeError";
import { Button } from "@/shared/ui/Button";
import { Input } from "@/shared/ui/Input";
import { Modal } from "@/shared/ui/Modal";
import { backupLabel } from "./backupLabel";
import {
  customBackupName,
  normalizeBackupName,
  validateBackupName,
} from "./backupName";

export interface RenameBackupModalProps {
  file: BackupFile | null;
  files: BackupFile[];
  busy: boolean;
  blocked: boolean;
  error: ErrorCopy | null;
  onOpenChange: (open: boolean) => void;
  onSubmit: (name: string) => void;
}

export function RenameBackupModal({
  file,
  files,
  busy,
  blocked,
  error,
  onOpenChange,
  onSubmit,
}: RenameBackupModalProps) {
  const { t, i18n } = useTranslation();
  const inputRef = useRef<HTMLInputElement>(null);
  const [name, setName] = useState("");
  const [touched, setTouched] = useState(false);

  useEffect(() => {
    setName(file ? (customBackupName(file) ?? "") : "");
    setTouched(false);
  }, [file]);

  const issue = file ? validateBackupName(name, file, files) : "required";
  const showIssue = touched && issue !== null;
  const label = file ? backupLabel(file, i18n.language) : "";

  return (
    <Modal
      open={file !== null}
      onOpenChange={onOpenChange}
      dismissible={!busy}
      initialFocusRef={inputRef}
      size="sm"
      title={t("preferences.backup.renameTitle", { name: label })}
      description={t("preferences.backup.renameDescription")}
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
            loading={busy}
            disabled={blocked || issue !== null}
            onClick={() => {
              setTouched(true);
              if (file === null || issue !== null) return;
              onSubmit(normalizeBackupName(name));
            }}
          >
            {t("preferences.backup.renameConfirm")}
          </Button>
        </>
      }
    >
      <form
        onSubmit={(event) => {
          event.preventDefault();
          setTouched(true);
          if (file === null || blocked || issue !== null) return;
          onSubmit(normalizeBackupName(name));
        }}
      >
        <label
          htmlFor="backup-rename-name"
          className="text-caption font-medium text-content"
        >
          {t("preferences.backup.nameLabel")}
        </label>
        <Input
          ref={inputRef}
          id="backup-rename-name"
          className="mt-2"
          value={name}
          disabled={busy}
          invalid={showIssue}
          aria-describedby="backup-rename-help"
          placeholder={t("preferences.backup.namePlaceholder")}
          onBlur={() => setTouched(true)}
          onChange={(event) => {
            setName(event.target.value);
            if (touched) setTouched(true);
          }}
        />
        <p
          id="backup-rename-help"
          className={`mt-2 text-caption ${showIssue ? "text-danger" : "text-content-muted"}`}
        >
          {showIssue
            ? t(`preferences.backup.renameValidation.${issue}`)
            : t("preferences.backup.nameHelp")}
        </p>
      </form>

      {error ? (
        <div
          role="alert"
          className="mt-3 flex items-start gap-2 rounded-md border border-danger/30 bg-danger/5 p-3"
        >
          <AlertCircle
            className="mt-0.5 h-4 w-4 shrink-0 text-danger"
            aria-hidden="true"
          />
          <p className="text-caption text-content">{t(error.messageKey)}</p>
        </div>
      ) : null}
    </Modal>
  );
}
