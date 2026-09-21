import { CheckCircle2 } from "lucide-react";
import { useEffect, useRef } from "react";
import { useTranslation } from "react-i18next";
import type { ToolId } from "@/entities/tool";
import { Button } from "@/shared/ui/Button";
import { Modal } from "@/shared/ui/Modal";
import { ExtensionMutationPausedNotice } from "./ExtensionMutationPausedNotice";
import { PromptMutationError } from "./PromptMutationError";
import { useImportPrompt } from "./usePromptMutations";

interface PromptImportModalProps {
  open: boolean;
  tool: ToolId;
  toolName: string;
  mutationsBlocked: boolean;
  onOpenChange: (open: boolean) => void;
}

export function PromptImportModal({
  open,
  tool,
  toolName,
  mutationsBlocked,
  onOpenChange,
}: PromptImportModalProps) {
  const { t } = useTranslation();
  const cancelRef = useRef<HTMLButtonElement>(null);
  const importPrompt = useImportPrompt();
  const { reset: resetImportPrompt } = importPrompt;

  useEffect(() => {
    if (open) resetImportPrompt();
  }, [open, resetImportPrompt]);

  return (
    <Modal
      open={open}
      onOpenChange={(next) => {
        if (!next && importPrompt.isPending) return;
        onOpenChange(next);
      }}
      title={t("extensions.prompt.import.title")}
      description={t("extensions.prompt.import.description", {
        tool: toolName,
      })}
      dismissible={!importPrompt.isPending}
      initialFocusRef={cancelRef}
      footer={
        <>
          <Button
            ref={cancelRef}
            variant="secondary"
            disabled={importPrompt.isPending}
            onClick={() => onOpenChange(false)}
          >
            {t("ds.action.cancel")}
          </Button>
          <Button
            loading={importPrompt.isPending}
            disabled={mutationsBlocked}
            onClick={() =>
              importPrompt.mutate(
                { tool },
                { onSuccess: () => onOpenChange(false) },
              )
            }
          >
            {t("extensions.prompt.import.confirm")}
          </Button>
        </>
      }
    >
      <div className="flex flex-col gap-4">
        {mutationsBlocked ? <ExtensionMutationPausedNotice /> : null}
        <ul className="flex flex-col gap-3 text-body text-content-muted">
          {["reads", "unchanged", "duplicate"].map((point) => (
            <li key={point} className="flex items-start gap-2">
              <CheckCircle2
                className="mt-0.5 h-4 w-4 shrink-0 text-brand"
                aria-hidden="true"
              />
              {t(`extensions.prompt.import.point.${point}`)}
            </li>
          ))}
        </ul>
        {importPrompt.isError ? (
          <PromptMutationError
            error={importPrompt.error}
            titleKey="extensions.prompt.import.errorTitle"
          />
        ) : null}
      </div>
    </Modal>
  );
}
