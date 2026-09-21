import { Trash2 } from "lucide-react";
import { useState } from "react";
import { useTranslation } from "react-i18next";
import type {
  SkillRepository,
  SkillRepositoryDraft,
} from "@/entities/skill-repository";
import { Button } from "@/shared/ui/Button";
import { Switch } from "@/shared/ui/Switch";

interface SkillRepositoryRowProps {
  repository: SkillRepository;
  busy: boolean;
  onSave: (draft: SkillRepositoryDraft) => void;
  onRemove: (id: string) => void;
}

export function SkillRepositoryRow({
  repository,
  busy,
  onSave,
  onRemove,
}: SkillRepositoryRowProps) {
  const { t } = useTranslation();
  const [confirming, setConfirming] = useState(false);
  const name = `${repository.owner}/${repository.repository}`;
  const branch =
    repository.branch === "" || repository.branch.toUpperCase() === "HEAD"
      ? t("extensions.skill.repositories.defaultBranch")
      : repository.branch;

  return (
    <li className="rounded-xl border border-hairline bg-layer-1 px-4 py-3 shadow-sm">
      <div className="flex min-w-0 items-center justify-between gap-4">
        <div className="min-w-0">
          <p className="break-words text-body font-medium text-content">
            {name}
          </p>
          <p className="mt-0.5 break-words text-caption text-content-muted">
            {t("extensions.skill.repositories.branchValue", { branch })}
          </p>
        </div>
        <div className="flex shrink-0 items-center gap-3">
          <Switch
            checked={repository.enabled}
            disabled={busy}
            aria-label={t(
              repository.enabled
                ? "extensions.skill.repositories.disableNamed"
                : "extensions.skill.repositories.enableNamed",
              { name },
            )}
            onCheckedChange={(enabled) =>
              onSave({
                owner: repository.owner,
                repository: repository.repository,
                branch: repository.branch,
                enabled,
              })
            }
          />
          <Button
            size="sm"
            variant="ghost"
            className="text-danger hover:bg-danger/10 hover:text-danger"
            disabled={busy}
            aria-label={t("extensions.skill.repositories.removeNamed", {
              name,
            })}
            onClick={() => setConfirming(true)}
          >
            <Trash2 className="h-4 w-4" aria-hidden="true" />
            {t("extensions.skill.repositories.remove")}
          </Button>
        </div>
      </div>

      {confirming ? (
        <div
          role="alertdialog"
          aria-label={t("extensions.skill.repositories.removeTitle", {
            name,
          })}
          className="mt-3 flex flex-col gap-3 border-t border-hairline pt-3 sm:flex-row sm:items-center sm:justify-between"
        >
          <p className="text-caption leading-5 text-content-muted">
            {t("extensions.skill.repositories.removeDescription")}
          </p>
          <div className="flex shrink-0 items-center gap-2">
            <Button
              size="sm"
              variant="ghost"
              disabled={busy}
              onClick={() => setConfirming(false)}
            >
              {t("ds.action.cancel")}
            </Button>
            <Button
              size="sm"
              variant="danger"
              disabled={busy}
              onClick={() => onRemove(repository.id)}
            >
              {t("extensions.skill.repositories.removeConfirm")}
            </Button>
          </div>
        </div>
      ) : null}
    </li>
  );
}
