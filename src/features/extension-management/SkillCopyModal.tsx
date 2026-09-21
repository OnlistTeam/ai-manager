import { AlertCircle, Check, CheckCircle2, Copy } from "lucide-react";
import { useEffect, useRef, useState } from "react";
import { useTranslation } from "react-i18next";
import type { Extension, ExtensionScope } from "@/entities/extension";
import type { ToolId } from "@/entities/tool";
import { toErrorCopy } from "@/shared/lib/nativeError";
import { Badge } from "@/shared/ui/Badge";
import { Button } from "@/shared/ui/Button";
import { Checkbox } from "@/shared/ui/Checkbox";
import { Modal } from "@/shared/ui/Modal";
import { ExtensionMutationPausedNotice } from "./ExtensionMutationPausedNotice";
import type { CopyDetectedSkillVariables } from "./useExtensionMutations";
import type { UseMutationResult } from "@tanstack/react-query";

export interface SkillCopyTarget {
  id: ToolId;
  name: string;
}

export interface SkillCopyModalProps {
  skill: Extension | null;
  scope: ExtensionScope;
  /** Tools that can hold skills, already filtered by `canManageSkills` and excluding the current tool. */
  targets: readonly SkillCopyTarget[];
  /** Tools on this machine that already have this skill. */
  ownedBy: ReadonlySet<ToolId>;
  mutationsBlocked: boolean;
  copy: UseMutationResult<Extension[], Error, CopyDetectedSkillVariables>;
  onOpenChange: (open: boolean) => void;
}

type CopyOutcome = "copied" | "failed";

/**
 * Copy tool by tool and report each result — no all-or-nothing. If the second
 * of three targets fails, the copies to the first and third are still real;
 * rolling all of them back together would only confuse the user more.
 */
export function SkillCopyModal({
  skill,
  scope,
  targets,
  ownedBy,
  mutationsBlocked,
  copy,
  onOpenChange,
}: SkillCopyModalProps) {
  const { t } = useTranslation();
  const cancelRef = useRef<HTMLButtonElement>(null);
  const [picked, setPicked] = useState<readonly ToolId[]>([]);
  const [outcomes, setOutcomes] = useState<ReadonlyMap<ToolId, CopyOutcome>>(
    new Map(),
  );
  const [running, setRunning] = useState(false);
  const skillId = skill?.id ?? null;
  const resetCopy = copy.reset;

  useEffect(() => {
    setPicked([]);
    setOutcomes(new Map());
    resetCopy();
  }, [resetCopy, skillId]);

  const available = targets.filter((target) => !ownedBy.has(target.id));
  const failure = copy.error ? toErrorCopy(copy.error) : null;
  const copiedCount = [...outcomes.values()].filter(
    (outcome) => outcome === "copied",
  ).length;

  async function runCopy() {
    if (skill === null || picked.length === 0 || mutationsBlocked) return;
    setRunning(true);
    const results = new Map(outcomes);
    for (const target of picked) {
      try {
        await copy.mutateAsync({ scope, target, skillId: skill.id });
        results.set(target, "copied");
      } catch {
        results.set(target, "failed");
      }
      setOutcomes(new Map(results));
    }
    setPicked([]);
    setRunning(false);
  }

  return (
    <Modal
      open={skill !== null}
      size="md"
      dismissible={!running}
      initialFocusRef={cancelRef}
      onOpenChange={onOpenChange}
      title={t("extensions.copy.title", { name: skill?.name ?? "" })}
      description={t("extensions.copy.description")}
      footer={
        <>
          <Button
            ref={cancelRef}
            variant="secondary"
            disabled={running}
            onClick={() => onOpenChange(false)}
          >
            {copiedCount > 0 ? t("ds.action.close") : t("ds.action.cancel")}
          </Button>
          <Button
            disabled={mutationsBlocked || picked.length === 0}
            loading={running}
            onClick={() => void runCopy()}
          >
            <Copy className="h-4 w-4" aria-hidden="true" />
            {t("extensions.copy.confirm")}
          </Button>
        </>
      }
    >
      {mutationsBlocked ? <ExtensionMutationPausedNotice /> : null}

      {available.length === 0 ? (
        <p className="text-body text-content-muted">
          {t("extensions.copy.noTargets")}
        </p>
      ) : (
        <ul
          aria-label={t("extensions.copy.targetsLabel")}
          className="flex flex-col gap-1"
        >
          {targets.map((target) => {
            const owned = ownedBy.has(target.id);
            const outcome = outcomes.get(target.id);
            const checkboxId = `skill-copy-${target.id}`;
            return (
              <li
                key={target.id}
                className="flex items-center justify-between gap-3 rounded-md px-2 py-2"
              >
                <div className="flex min-w-0 items-center gap-2.5">
                  <Checkbox
                    id={checkboxId}
                    checked={picked.includes(target.id)}
                    disabled={owned || running || outcome === "copied"}
                    onCheckedChange={(checked) =>
                      setPicked((current) =>
                        checked
                          ? [...current, target.id]
                          : current.filter((id) => id !== target.id),
                      )
                    }
                  />
                  <label
                    htmlFor={checkboxId}
                    className="min-w-0 truncate text-body text-content"
                  >
                    {target.name}
                  </label>
                </div>
                {owned ? (
                  <Badge tone="neutral">
                    {t("extensions.copy.alreadyHas")}
                  </Badge>
                ) : outcome === "copied" ? (
                  <Badge tone="success" icon={Check}>
                    {t("extensions.copy.copied")}
                  </Badge>
                ) : outcome === "failed" ? (
                  <Badge tone="danger">{t("extensions.copy.failed")}</Badge>
                ) : null}
              </li>
            );
          })}
        </ul>
      )}

      {copiedCount > 0 ? (
        <p
          role="status"
          className="mt-3 flex items-start gap-2 rounded-md border border-success/20 bg-success/5 p-3 text-caption text-content-muted"
        >
          <CheckCircle2
            className="mt-0.5 h-4 w-4 shrink-0 text-success"
            aria-hidden="true"
          />
          {t("extensions.copy.independent")}
        </p>
      ) : null}

      {failure ? (
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
              {t(failure.messageKey)}
            </p>
            {failure.remediationKey ? (
              <p className="mt-0.5 text-caption leading-5 text-content-muted">
                {t(failure.remediationKey)}
              </p>
            ) : null}
          </div>
        </div>
      ) : null}
    </Modal>
  );
}
