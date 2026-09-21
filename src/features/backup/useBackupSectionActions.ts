import { useState } from "react";
import { useTranslation } from "react-i18next";
import type { BackupFile } from "@/entities/backup";
import { toErrorCopy } from "@/shared/lib/nativeError";
import { backupLabel } from "./backupLabel";
import type { BackupOutcome } from "./BackupOutcomeNotice";
import type { BackupAction } from "./ConfirmBackupActionModal";
import {
  useCreateBackup,
  useDeleteBackup,
  useExportBackupArchive,
  useImportBackupArchive,
  useRenameBackup,
  useRestoreBackup,
} from "./useBackupMutations";

interface Pending {
  action: BackupAction;
  file: BackupFile;
}

export function useBackupSectionActions(inventoryActionsBlocked: boolean) {
  const { i18n } = useTranslation();
  const create = useCreateBackup();
  const restore = useRestoreBackup();
  const remove = useDeleteBackup();
  const rename = useRenameBackup();
  const exportArchive = useExportBackupArchive();
  const importArchive = useImportBackupArchive();
  const [pending, setPending] = useState<Pending | null>(null);
  const [renaming, setRenaming] = useState<BackupFile | null>(null);
  const [importOpen, setImportOpen] = useState(false);
  const [outcome, setOutcome] = useState<BackupOutcome | null>(null);

  const busy =
    create.isPending ||
    restore.isPending ||
    remove.isPending ||
    rename.isPending ||
    exportArchive.isPending ||
    importArchive.isPending;
  const actionsBlocked = busy || inventoryActionsBlocked;
  const createError = create.isError ? toErrorCopy(create.error) : null;
  const pendingError =
    pending?.action === "restore" && restore.isError
      ? toErrorCopy(restore.error)
      : pending?.action === "delete" && remove.isError
        ? toErrorCopy(remove.error)
        : null;
  const renameError = rename.isError ? toErrorCopy(rename.error) : null;
  const exportError = exportArchive.isError
    ? toErrorCopy(exportArchive.error)
    : null;
  const importError = importArchive.isError
    ? toErrorCopy(importArchive.error)
    : null;

  const createBackup = () => {
    if (actionsBlocked) return;
    setOutcome(null);
    create.mutate(undefined, {
      onSuccess: () => setOutcome({ kind: "created" }),
    });
  };

  const openAction = (action: BackupAction, file: BackupFile) => {
    if (actionsBlocked) return;
    create.reset();
    restore.reset();
    remove.reset();
    rename.reset();
    setRenaming(null);
    setOutcome(null);
    setPending({ action, file });
  };

  const closeAction = () => {
    setPending(null);
    restore.reset();
    remove.reset();
  };

  const openRename = (file: BackupFile) => {
    if (actionsBlocked) return;
    create.reset();
    restore.reset();
    remove.reset();
    rename.reset();
    setPending(null);
    setOutcome(null);
    setRenaming(file);
  };

  const closeRename = () => {
    if (rename.isPending) return;
    setRenaming(null);
    rename.reset();
  };

  const confirmPending = () => {
    if (pending === null || actionsBlocked) return;
    setOutcome(null);
    const label = backupLabel(pending.file, i18n.language);
    if (pending.action === "restore") {
      restore.mutate(pending.file.name, {
        onSuccess: (restored) => {
          setOutcome({
            kind: "restored",
            label,
            toolsOutOfSync: restored.toolsOutOfSync,
          });
          setPending(null);
        },
      });
      return;
    }
    remove.mutate(pending.file.name, {
      onSuccess: () => {
        setOutcome({ kind: "deleted", label });
        setPending(null);
      },
    });
  };

  const submitRename = (name: string) => {
    if (renaming === null || actionsBlocked) return;
    setOutcome(null);
    rename.mutate(
      { source: renaming.name, name },
      {
        onSuccess: () => {
          setOutcome({ kind: "renamed", label: name });
          setRenaming(null);
        },
      },
    );
  };

  const exportConfiguration = () => {
    if (actionsBlocked) return;
    importArchive.reset();
    setPending(null);
    setRenaming(null);
    setOutcome(null);
    exportArchive.mutate(undefined, {
      onSuccess: (result) => {
        if (result.status === "exported") setOutcome({ kind: "exported" });
      },
    });
  };

  const openImport = () => {
    if (actionsBlocked) return;
    exportArchive.reset();
    importArchive.reset();
    setPending(null);
    setRenaming(null);
    setOutcome(null);
    setImportOpen(true);
  };

  const closeImport = () => {
    if (importArchive.isPending) return;
    setImportOpen(false);
    importArchive.reset();
  };

  const importConfiguration = () => {
    if (actionsBlocked) return;
    exportArchive.reset();
    importArchive.mutate(undefined, {
      onSuccess: (result) => {
        if (result.status !== "imported") return;
        setOutcome({
          kind: "imported",
          toolsOutOfSync: result.toolsOutOfSync,
        });
        setImportOpen(false);
      },
    });
  };

  return {
    actionsBlocked,
    busy,
    create,
    createError,
    pending,
    pendingError,
    renaming,
    rename,
    renameError,
    exportArchive,
    importArchive,
    exportError,
    importError,
    importOpen,
    outcome,
    createBackup,
    openAction,
    closeAction,
    openRename,
    closeRename,
    confirmPending,
    submitRename,
    exportConfiguration,
    openImport,
    closeImport,
    importConfiguration,
  };
}
