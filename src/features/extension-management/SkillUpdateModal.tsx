import { AlertCircle, RefreshCw } from "lucide-react";
import { useEffect, useRef } from "react";
import { useTranslation } from "react-i18next";
import type { Extension } from "@/entities/extension";
import { toErrorCopy } from "@/shared/lib/nativeError";
import { Button } from "@/shared/ui/Button";
import { Modal } from "@/shared/ui/Modal";
import { ExtensionMutationPausedNotice } from "./ExtensionMutationPausedNotice";
import { useUpdateSkill } from "./useSkillUpdate";

const IMPACT_KEYS = [
  "extensions.skill.update.point.download",
  "extensions.skill.update.point.backup",
  "extensions.skill.update.point.tools",
] as const;

export interface SkillUpdateModalProps {
  skill: Extension | null;
  mutationsBlocked: boolean;
  onOpenChange: (open: boolean) => void;
}

export function SkillUpdateModal({
  skill,
  mutationsBlocked,
  onOpenChange,
}: SkillUpdateModalProps) {
  const { t } = useTranslation();
  const cancelRef = useRef<HTMLButtonElement>(null);
  const update = useUpdateSkill();
  const resetUpdate = update.reset;
  const error = update.error ? toErrorCopy(update.error) : null;

  useEffect(() => {
    resetUpdate();
  }, [resetUpdate, skill?.id]);

  return (
    <Modal
      open={skill !== null}
      size="md"
      dismissible={!update.isPending}
      initialFocusRef={cancelRef}
      onOpenChange={onOpenChange}
      title={t("extensions.skill.update.title", { name: skill?.name ?? "" })}
      description={t("extensions.skill.update.description")}
      footer={
        <>
          <Button
            ref={cancelRef}
            variant="secondary"
            disabled={update.isPending}
            onClick={() => onOpenChange(false)}
          >
            {t("ds.action.cancel")}
          </Button>
          <Button
            disabled={mutationsBlocked}
            loading={update.isPending}
            onClick={() => {
              if (!skill || skill.scope.kind !== "tool" || mutationsBlocked)
                return;
              update.mutate(
                { tool: skill.scope.id, skillId: skill.id, name: skill.name },
                { onSuccess: () => onOpenChange(false) },
              );
            }}
          >
            <RefreshCw className="h-4 w-4" aria-hidden="true" />
            {t("extensions.skill.update.confirm")}
          </Button>
        </>
      }
    >
      {mutationsBlocked ? <ExtensionMutationPausedNotice /> : null}

      <ul className="flex flex-col gap-2 rounded-md bg-brand/10 p-3">
        {IMPACT_KEYS.map((key) => (
          <li
            key={key}
            className="flex items-start gap-2 text-caption text-content"
          >
            <RefreshCw
              className="mt-0.5 h-4 w-4 shrink-0 text-brand"
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
              {t("extensions.skill.update.errorRetry")}
            </p>
          </div>
        </div>
      ) : null}
    </Modal>
  );
}
