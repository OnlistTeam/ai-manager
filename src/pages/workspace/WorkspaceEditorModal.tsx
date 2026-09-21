import { AlertCircle, ShieldCheck } from "lucide-react";
import { useEffect, useRef, useState } from "react";
import { useTranslation } from "react-i18next";
import { toast } from "sonner";
import {
  MAX_OPENCLAW_WORKSPACE_CONTENT_BYTES,
  useOpenClawDailyMemory,
  useOpenClawWorkspaceDocument,
  useSaveOpenClawDailyMemory,
  useSaveOpenClawWorkspaceDocument,
  type OpenClawWorkspaceFileId,
} from "@/entities/openclaw-workspace";
import { toErrorCopy } from "@/shared/lib/nativeError";
import { Button } from "@/shared/ui/Button";
import { DetectionStatus } from "@/shared/ui/DetectionStatus";
import { Modal } from "@/shared/ui/Modal";
import { Textarea } from "@/shared/ui/Textarea";

export type WorkspaceEditorTarget =
  | { kind: "file"; id: OpenClawWorkspaceFileId; filename: string }
  | { kind: "memory"; date: string };

interface WorkspaceEditorModalProps {
  target: WorkspaceEditorTarget | null;
  onOpenChange: (open: boolean) => void;
}

export function WorkspaceEditorModal({
  target,
  onOpenChange,
}: WorkspaceEditorModalProps) {
  const { t } = useTranslation();
  const textareaRef = useRef<HTMLTextAreaElement>(null);
  const hydrated = useRef<string | null>(null);
  const [content, setContent] = useState("");
  const file = useOpenClawWorkspaceDocument(
    target?.kind === "file" ? target.id : null,
  );
  const memory = useOpenClawDailyMemory(
    target?.kind === "memory" ? target.date : null,
  );
  const saveFile = useSaveOpenClawWorkspaceDocument();
  const saveMemory = useSaveOpenClawDailyMemory();
  const { reset: resetSaveFile } = saveFile;
  const { reset: resetSaveMemory } = saveMemory;
  const query = target?.kind === "file" ? file : memory;
  const save = target?.kind === "file" ? saveFile : saveMemory;
  const data = query.data;
  const targetKey = target
    ? target.kind === "file"
      ? `file:${target.id}`
      : `memory:${target.date}`
    : null;

  useEffect(() => {
    if (targetKey === null) return;
    hydrated.current = null;
    setContent("");
    resetSaveFile();
    resetSaveMemory();
  }, [targetKey, resetSaveFile, resetSaveMemory]);

  useEffect(() => {
    if (!data || targetKey === null || hydrated.current === targetKey) return;
    hydrated.current = targetKey;
    setContent(data.content);
  }, [data, targetKey]);

  const bytes = new TextEncoder().encode(content).byteLength;
  const tooLarge = bytes > MAX_OPENCLAW_WORKSPACE_CONTENT_BYTES;
  const dirty = data !== undefined && content !== data.content;
  const error = query.error ?? save.error;
  const errorCopy = error ? toErrorCopy(error) : null;
  const title =
    target?.kind === "file"
      ? t("openClawWorkspace.editor.fileTitle", { filename: target.filename })
      : t("openClawWorkspace.editor.memoryTitle", { date: target?.date ?? "" });

  function submit(): void {
    if (!target || tooLarge || !dirty) return;
    const options = {
      onSuccess: (outcome: { backupCreated: boolean }) => {
        toast.success(t("openClawWorkspace.editor.saved"), {
          description: outcome.backupCreated
            ? t("openClawWorkspace.editor.backupCreated")
            : undefined,
        });
        onOpenChange(false);
      },
    };
    if (target.kind === "file") {
      saveFile.mutate({ file: target.id, content }, options);
    } else {
      saveMemory.mutate({ date: target.date, content }, options);
    }
  }

  return (
    <Modal
      open={target !== null}
      onOpenChange={(open) => {
        if (!open && !save.isPending) onOpenChange(false);
      }}
      title={title}
      description={t("openClawWorkspace.editor.description")}
      size="lg"
      dismissible={!save.isPending}
      initialFocusRef={textareaRef}
      footer={
        <>
          <Button
            variant="secondary"
            disabled={save.isPending}
            onClick={() => onOpenChange(false)}
          >
            {t("ds.action.cancel")}
          </Button>
          <Button
            loading={save.isPending}
            disabled={query.isPending || query.isError || tooLarge || !dirty}
            onClick={submit}
          >
            {t("openClawWorkspace.editor.save")}
          </Button>
        </>
      }
    >
      {query.isPending ? (
        <DetectionStatus label={t("openClawWorkspace.editor.loading")} />
      ) : null}

      {errorCopy ? (
        <div
          role="alert"
          className="mb-4 flex items-start gap-3 rounded-lg border border-danger/25 bg-danger/5 p-4"
        >
          <AlertCircle
            className="mt-0.5 h-4 w-4 shrink-0 text-danger"
            aria-hidden="true"
          />
          <div className="min-w-0 flex-1">
            <p className="text-body font-medium text-content">
              {t(errorCopy.messageKey)}
            </p>
            {query.isError ? (
              <Button
                className="mt-3"
                size="sm"
                variant="secondary"
                loading={query.isFetching}
                onClick={() => void query.refetch()}
              >
                {t("openClawWorkspace.retry")}
              </Button>
            ) : null}
          </div>
        </div>
      ) : null}

      <div className="flex items-start gap-2 rounded-lg border border-success/20 bg-success/5 p-3 text-caption text-content-muted">
        <ShieldCheck
          className="mt-0.5 h-4 w-4 shrink-0 text-success"
          aria-hidden="true"
        />
        {t("openClawWorkspace.editor.safety")}
      </div>
      <Textarea
        ref={textareaRef}
        className="mt-4 min-h-[340px] resize-y font-mono text-caption leading-6"
        value={content}
        disabled={query.isPending || query.isError || save.isPending}
        invalid={tooLarge}
        spellCheck={false}
        aria-label={t("openClawWorkspace.editor.contentLabel")}
        aria-describedby="openclaw-workspace-editor-size"
        onChange={(event) => setContent(event.target.value)}
      />
      <p
        id="openclaw-workspace-editor-size"
        className={`mt-2 text-caption ${tooLarge ? "text-danger" : "text-content-muted"}`}
      >
        {t(
          tooLarge
            ? "openClawWorkspace.editor.tooLarge"
            : "openClawWorkspace.editor.size",
          { current: bytes, maximum: MAX_OPENCLAW_WORKSPACE_CONTENT_BYTES },
        )}
      </p>
    </Modal>
  );
}
