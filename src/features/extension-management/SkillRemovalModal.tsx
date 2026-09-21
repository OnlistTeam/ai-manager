import { AlertCircle, AlertTriangle } from "lucide-react";
import { useEffect, useRef } from "react";
import { useTranslation } from "react-i18next";
import type { Extension } from "@/entities/extension";
import { toErrorCopy } from "@/shared/lib/nativeError";
import { Button } from "@/shared/ui/Button";
import { Modal } from "@/shared/ui/Modal";
import { ExtensionMutationPausedNotice } from "./ExtensionMutationPausedNotice";
import { useRemoveSkill } from "./useSkillRemoval";

const IMPACT_KEYS = [
  "extensions.skill.remove.point.everywhere",
  "extensions.skill.remove.point.recovery",
  "extensions.skill.remove.point.external",
] as const;

export interface SkillRemovalModalProps {
  skill: Extension | null;
  mutationsBlocked: boolean;
  onOpenChange: (open: boolean) => void;
}

export function SkillRemovalModal({
  skill,
  mutationsBlocked,
  onOpenChange,
}: SkillRemovalModalProps) {
  const { t } = useTranslation();
  const cancelRef = useRef<HTMLButtonElement>(null);
  const remove = useRemoveSkill();
  const resetRemoval = remove.reset;
  const error = remove.error ? toErrorCopy(remove.error) : null;

  useEffect(() => {
    resetRemoval();
  }, [resetRemoval, skill?.id]);

  return (
    <Modal
      open={skill !== null}
      size="md"
      dismissible={!remove.isPending}
      initialFocusRef={cancelRef}
      onOpenChange={onOpenChange}
      title={t("extensions.skill.remove.title", { name: skill?.name ?? "" })}
      description={t("extensions.skill.remove.description")}
      footer={
        <>
          <Button
            ref={cancelRef}
            variant="secondary"
            disabled={remove.isPending}
            onClick={() => onOpenChange(false)}
          >
            {t("ds.action.cancel")}
          </Button>
          <Button
            variant="danger"
            disabled={mutationsBlocked}
            loading={remove.isPending}
            onClick={() => {
              if (!skill || skill.scope.kind !== "tool" || mutationsBlocked)
                return;
              remove.mutate(
                { tool: skill.scope.id, skillId: skill.id, name: skill.name },
                { onSuccess: () => onOpenChange(false) },
              );
            }}
          >
            {t("extensions.skill.remove.confirm")}
          </Button>
        </>
      }
    >
      {mutationsBlocked ? <ExtensionMutationPausedNotice /> : null}

      <ul className="flex flex-col gap-2 rounded-md bg-danger/10 p-3">
        {IMPACT_KEYS.map((key) => (
          <li
            key={key}
            className="flex items-start gap-2 text-caption text-content"
          >
            <AlertTriangle
              className="mt-0.5 h-4 w-4 shrink-0 text-danger"
              aria-hidden="true"
            />
            {t(key)}
          </li>
        ))}
      </ul>

      {error ? (
        <div
          role="alert"
          className="mt-3 flex items-start gap-2 rounded-md border border-danger/30 bg-danger/5 p-3"
        >
          <AlertCircle
            className="mt-0.5 h-4 w-4 shrink-0 text-danger"
            aria-hidden="true"
          />
          <div>
            <p className="text-caption font-medium text-content">
              {t(error.messageKey)}
            </p>
            <p className="mt-0.5 text-caption leading-5 text-content-muted">
              {t("extensions.skill.remove.errorRetry")}
            </p>
          </div>
        </div>
      ) : null}
    </Modal>
  );
}
