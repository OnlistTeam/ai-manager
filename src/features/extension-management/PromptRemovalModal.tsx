import { useEffect, useRef } from "react";
import { useTranslation } from "react-i18next";
import type { Extension } from "@/entities/extension";
import { Button } from "@/shared/ui/Button";
import { Modal } from "@/shared/ui/Modal";
import { ExtensionMutationPausedNotice } from "./ExtensionMutationPausedNotice";
import { PromptMutationError } from "./PromptMutationError";
import { useRemovePrompt } from "./usePromptMutations";

interface PromptRemovalModalProps {
  prompt: Extension | null;
  mutationsBlocked: boolean;
  onOpenChange: (open: boolean) => void;
}

export function PromptRemovalModal({
  prompt,
  mutationsBlocked,
  onOpenChange,
}: PromptRemovalModalProps) {
  const { t } = useTranslation();
  const cancelRef = useRef<HTMLButtonElement>(null);
  const removePrompt = useRemovePrompt();
  const { reset: resetRemovePrompt } = removePrompt;
  const promptId = prompt?.id ?? null;

  useEffect(() => {
    if (promptId !== null) resetRemovePrompt();
  }, [promptId, resetRemovePrompt]);

  if (!prompt) return null;
  const active = prompt.enabled;
  const tool = prompt.scope.kind === "tool" ? prompt.scope.id : null;

  return (
    <Modal
      open
      onOpenChange={(next) => {
        if (!next && removePrompt.isPending) return;
        onOpenChange(next);
      }}
      title={t("extensions.prompt.remove.title", { name: prompt.name })}
      description={t("extensions.prompt.remove.description")}
      dismissible={!removePrompt.isPending}
      initialFocusRef={cancelRef}
      footer={
        <>
          <Button
            ref={cancelRef}
            variant="secondary"
            disabled={removePrompt.isPending}
            onClick={() => onOpenChange(false)}
          >
            {t("ds.action.cancel")}
          </Button>
          <Button
            variant="danger"
            loading={removePrompt.isPending}
            disabled={mutationsBlocked || active || tool === null}
            onClick={() =>
              tool === null
                ? undefined
                : removePrompt.mutate(
                    { tool, promptId: prompt.id },
                    { onSuccess: () => onOpenChange(false) },
                  )
            }
          >
            {t("extensions.prompt.remove.confirm")}
          </Button>
        </>
      }
    >
      <div className="flex flex-col gap-4">
        {mutationsBlocked ? <ExtensionMutationPausedNotice /> : null}
        {active ? (
          <p role="alert" className="text-caption text-danger">
            {t("extensions.prompt.remove.active")}
          </p>
        ) : null}
        {removePrompt.isError ? (
          <PromptMutationError
            error={removePrompt.error}
            titleKey="extensions.prompt.remove.errorTitle"
          />
        ) : null}
      </div>
    </Modal>
  );
}
