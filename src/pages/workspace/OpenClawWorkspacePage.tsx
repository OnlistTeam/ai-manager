import { AlertCircle, BrainCircuit, FolderOpen, RefreshCw } from "lucide-react";
import { useState } from "react";
import { useTranslation } from "react-i18next";
import {
  useOpenClawWorkspaceDirectory,
  useOpenClawWorkspaceOverview,
} from "@/entities/openclaw-workspace";
import { toErrorCopy } from "@/shared/lib/nativeError";
import { Button } from "@/shared/ui/Button";
import { Card } from "@/shared/ui/Card";
import { DetectionStatus } from "@/shared/ui/DetectionStatus";
import { EmptyState } from "@/shared/ui/EmptyState";
import { SectionHeader } from "@/shared/ui/SectionHeader";
import { DailyMemorySection } from "./DailyMemorySection";
import {
  WorkspaceEditorModal,
  type WorkspaceEditorTarget,
} from "./WorkspaceEditorModal";
import { WorkspaceFilesGrid } from "./WorkspaceFilesGrid";
import { WorkspaceOverviewStrip } from "./WorkspaceOverviewStrip";

export function OpenClawWorkspacePage() {
  const { t } = useTranslation();
  const overview = useOpenClawWorkspaceOverview();
  const openDirectory = useOpenClawWorkspaceDirectory();
  const [editor, setEditor] = useState<WorkspaceEditorTarget | null>(null);
  const error = overview.error ?? openDirectory.error;
  const errorCopy = error ? toErrorCopy(error) : null;
  const initiallyLoading = overview.isPending && !overview.isFetched;
  const unavailable = overview.isFetched && overview.data === undefined;

  return (
    <div
      role="region"
      aria-label={t("openClawWorkspace.title")}
      className="flex min-w-0 flex-col gap-6"
    >
      <SectionHeader
        as="h2"
        title={t("openClawWorkspace.title")}
        description={t("openClawWorkspace.description")}
        action={
          <Button
            size="sm"
            variant="secondary"
            loading={openDirectory.isPending}
            onClick={() => openDirectory.mutate("workspace")}
          >
            <FolderOpen className="h-4 w-4" aria-hidden="true" />
            {t("openClawWorkspace.openFolder")}
          </Button>
        }
      />

      {initiallyLoading ? (
        <DetectionStatus label={t("openClawWorkspace.loading")} />
      ) : null}

      {unavailable ? (
        <EmptyState
          icon={BrainCircuit}
          title={t("openClawWorkspace.error.title")}
          description={t("openClawWorkspace.error.description")}
          action={
            <Button
              loading={overview.isFetching}
              onClick={() => void overview.refetch()}
            >
              <RefreshCw className="h-4 w-4" aria-hidden="true" />
              {t("openClawWorkspace.retry")}
            </Button>
          }
        />
      ) : null}

      {errorCopy && overview.data ? (
        <Card
          role="alert"
          className="flex items-start gap-3 border-warning/25 bg-warning/5"
        >
          <AlertCircle
            className="mt-0.5 h-4 w-4 shrink-0 text-warning"
            aria-hidden="true"
          />
          <p className="min-w-0 flex-1 text-caption text-content-muted">
            {t(errorCopy.messageKey)}
          </p>
          <Button
            size="sm"
            variant="secondary"
            loading={overview.isFetching}
            onClick={() => void overview.refetch()}
          >
            {t("openClawWorkspace.retry")}
          </Button>
        </Card>
      ) : null}

      {overview.data ? (
        <>
          <WorkspaceOverviewStrip overview={overview.data} />
          <WorkspaceFilesGrid
            files={overview.data.files}
            onEdit={(file) =>
              setEditor({ kind: "file", id: file.id, filename: file.filename })
            }
          />
          <DailyMemorySection onEdit={setEditor} />
        </>
      ) : null}

      <WorkspaceEditorModal
        target={editor}
        onOpenChange={(open) => {
          if (!open) setEditor(null);
        }}
      />
    </div>
  );
}
