import { Archive, Download, Upload } from "lucide-react";
import { useTranslation } from "react-i18next";
import { Button } from "@/shared/ui/Button";
import { SectionHeader } from "@/shared/ui/SectionHeader";
import { backupLabel } from "./backupLabel";
import { BackupActionError } from "./BackupActionError";
import { BackupImportModal } from "./BackupImportModal";
import {
  BackupInventoryNotice,
  BackupInventorySkeleton,
  BackupInventoryUnavailable,
} from "./BackupInventoryNotice";
import { BackupListCard } from "./BackupListCard";
import { BackupOutcomeNotice } from "./BackupOutcomeNotice";
import { BackupScheduleRow } from "./BackupScheduleRow";
import { ConfirmBackupActionModal } from "./ConfirmBackupActionModal";
import { FirstBackupCard } from "./FirstBackupCard";
import { RenameBackupModal } from "./RenameBackupModal";
import { useBackupInventoryRecovery } from "./useBackupInventoryRecovery";
import { useBackupSectionActions } from "./useBackupSectionActions";

export interface BackupSectionProps {
  /** Legacy caller compatibility; technical details now use disclosure. */
  advancedMode?: boolean;
  /** Restore warnings can send the user straight to the one place that fixes them. */
  onReviewServices?: () => void;
}

export function BackupSection({ onReviewServices }: BackupSectionProps) {
  const { t, i18n } = useTranslation();
  const {
    backups,
    sectionRef,
    retryButtonRef,
    inventoryAvailable,
    inventoryInitiallyLoading,
    inventoryUnavailable,
    inventoryRefreshFailed,
    inventoryActionsBlocked,
    retryInventory,
  } = useBackupInventoryRecovery();
  const actions = useBackupSectionActions(inventoryActionsBlocked);
  const files = backups.data?.files ?? [];
  const hasBackups = inventoryAvailable && files.length > 0;
  const hasNoBackups = inventoryAvailable && files.length === 0;

  return (
    <section
      ref={sectionRef}
      aria-label={t("preferences.backup.title")}
      tabIndex={-1}
      className="flex flex-col gap-4 outline-none"
    >
      <SectionHeader
        as="h3"
        title={t("preferences.backup.title")}
        action={
          hasBackups ? (
            <Button
              variant="secondary"
              loading={actions.create.isPending}
              disabled={actions.actionsBlocked}
              onClick={actions.createBackup}
            >
              {!actions.create.isPending ? (
                <Archive className="h-4 w-4" aria-hidden="true" />
              ) : null}
              {t("preferences.backup.create")}
            </Button>
          ) : undefined
        }
      />

      <BackupScheduleRow />

      {inventoryInitiallyLoading ? <BackupInventorySkeleton /> : null}

      {inventoryUnavailable ? (
        <BackupInventoryUnavailable
          refreshing={backups.isFetching}
          retryButtonRef={retryButtonRef}
          onRetry={retryInventory}
        />
      ) : null}

      {inventoryRefreshFailed ? (
        <BackupInventoryNotice
          refreshing={backups.isFetching}
          retryButtonRef={retryButtonRef}
          onRetry={retryInventory}
        />
      ) : null}

      {actions.createError ? (
        <BackupActionError
          error={actions.createError}
          disabled={actions.actionsBlocked}
          onRetry={actions.createBackup}
        />
      ) : null}

      {actions.exportError ? (
        <BackupActionError
          error={actions.exportError}
          disabled={actions.actionsBlocked}
          onRetry={actions.exportConfiguration}
        />
      ) : null}

      {actions.outcome ? (
        <BackupOutcomeNotice
          outcome={actions.outcome}
          onReviewServices={onReviewServices}
        />
      ) : null}

      {hasNoBackups ? (
        <FirstBackupCard
          creating={actions.create.isPending}
          disabled={actions.actionsBlocked}
          onCreate={actions.createBackup}
        />
      ) : null}

      {hasBackups ? (
        <BackupListCard
          files={files}
          busy={actions.actionsBlocked}
          onRename={actions.openRename}
          onRestore={(file) => actions.openAction("restore", file)}
          onDelete={(file) => actions.openAction("delete", file)}
        />
      ) : null}

      {inventoryAvailable ? (
        <div className="flex flex-wrap gap-2">
          <Button
            variant="secondary"
            loading={actions.exportArchive.isPending}
            disabled={actions.actionsBlocked}
            onClick={actions.exportConfiguration}
          >
            {!actions.exportArchive.isPending ? (
              <Download className="h-4 w-4" aria-hidden="true" />
            ) : null}
            {t("preferences.backup.exportToOther")}
          </Button>
          <Button
            variant="secondary"
            disabled={actions.actionsBlocked}
            onClick={actions.openImport}
          >
            <Upload className="h-4 w-4" aria-hidden="true" />
            {t("preferences.backup.importFromOther")}
          </Button>
        </div>
      ) : null}

      {inventoryAvailable ? (
        <details className="rounded-lg border border-hairline bg-layer-1 px-3 py-2">
          <summary className="cursor-pointer list-none text-caption text-content-muted">
            {t("preferences.backup.details")}
          </summary>
          <p className="mt-2 break-words border-t border-hairline pt-2 text-mono-sm text-content-muted">
            {t("preferences.backup.folder")}
          </p>
        </details>
      ) : null}

      <ConfirmBackupActionModal
        action={actions.pending?.action ?? null}
        label={
          actions.pending
            ? backupLabel(actions.pending.file, i18n.language)
            : ""
        }
        busy={actions.busy && actions.pending !== null}
        confirmDisabled={inventoryActionsBlocked || actions.create.isPending}
        error={actions.pendingError}
        onOpenChange={(open) => {
          if (!open) actions.closeAction();
        }}
        onConfirm={actions.confirmPending}
      />

      <RenameBackupModal
        file={actions.renaming}
        files={files}
        busy={actions.rename.isPending}
        blocked={inventoryActionsBlocked}
        error={actions.renameError}
        onOpenChange={(open) => {
          if (!open) actions.closeRename();
        }}
        onSubmit={actions.submitRename}
      />

      <BackupImportModal
        open={actions.importOpen}
        blocked={inventoryActionsBlocked}
        importing={actions.importArchive.isPending}
        error={actions.importError}
        onOpenChange={(open) => {
          if (!open) actions.closeImport();
        }}
        onImport={actions.importConfiguration}
      />
    </section>
  );
}
