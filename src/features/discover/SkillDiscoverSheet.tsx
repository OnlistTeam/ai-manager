import { useState } from "react";
import { BadgeCheck } from "lucide-react";
import { useTranslation } from "react-i18next";
import type { DiscoverSkill } from "@/entities/discover";
import type { ExtensionScopeOption } from "@/features/extension-management";
import { toErrorCopy } from "@/shared/lib/nativeError";
import { Badge } from "@/shared/ui/Badge";
import { Button } from "@/shared/ui/Button";
import { Modal } from "@/shared/ui/Modal";
import { DiscoverAppChoice } from "./DiscoverAppChoice";
import { DiscoverSheetHead } from "./DiscoverSheetHead";
import { compactCount } from "./discoverLabels";
import type { DiscoverAddState } from "./useDiscoverAdds";
import { useOpenDiscoverLink } from "./useDiscoverMutations";

export interface SkillDiscoverSheetProps {
  skill: DiscoverSkill;
  description: string | null;
  describing: boolean;
  targets: readonly ExtensionScopeOption[];
  state: DiscoverAddState;
  blocked: boolean;
  onClose: () => void;
  /** Resolves to `null` when the task started, else to the refusal. */
  onAdd: (targets: readonly ExtensionScopeOption[]) => Promise<unknown>;
}

/** One Skill up close: where it comes from, what it does, which apps get it. */
export function SkillDiscoverSheet({
  skill,
  description,
  describing,
  targets,
  state,
  blocked,
  onClose,
  onAdd,
}: SkillDiscoverSheetProps) {
  const { t, i18n } = useTranslation();
  const openLink = useOpenDiscoverLink();
  const [selected, setSelected] = useState<ReadonlySet<string>>(
    () => new Set(targets.map((target) => target.key)),
  );
  const [refusal, setRefusal] = useState<unknown>(null);
  const [submitting, setSubmitting] = useState(false);
  // While this dialog's own request is out, the form stays; once a task
  // runs, or the item is already here, there is nothing left to fill in.
  const done = state === "added" || (state === "adding" && !submitting);
  const open = (link: "page" | "repository") =>
    openLink.mutate({ kind: "skill", id: skill.id, link });

  async function submit() {
    setSubmitting(true);
    setRefusal(null);
    const refused = await onAdd(
      targets.filter((target) => selected.has(target.key)),
    );
    setSubmitting(false);
    if (refused === null) onClose();
    else setRefusal(refused);
  }

  const refusalCopy = refusal ? toErrorCopy(refusal) : null;

  return (
    <Modal
      open
      onOpenChange={(next) => {
        if (!next) onClose();
      }}
      title={skill.name}
      dismissible={!submitting}
      footer={
        <>
          <Button variant="secondary" onClick={onClose} disabled={submitting}>
            {done ? t("discover.sheet.close") : t("discover.sheet.cancel")}
          </Button>
          {done ? null : (
            <Button
              loading={submitting}
              disabled={blocked || selected.size === 0}
              onClick={() => void submit()}
            >
              {t("discover.sheet.confirm")}
            </Button>
          )}
        </>
      }
    >
      <div className="flex flex-col gap-3">
        <DiscoverSheetHead
          icon={skill.icon}
          name={skill.source}
          meta={
            <>
              <span>{skill.source}</span>
              <Badge>
                {t("discover.installs", {
                  value: compactCount(skill.installs, i18n.language),
                })}
              </Badge>
              {skill.official ? (
                <Badge tone="brand" icon={BadgeCheck}>
                  {t("discover.official")}
                </Badge>
              ) : null}
            </>
          }
          links={[
            { label: t("discover.skillsPage"), onOpen: () => open("page") },
            {
              label: t("discover.repository"),
              onOpen: () => open("repository"),
            },
          ]}
        />
        <p
          className="text-body text-content"
          aria-busy={describing || undefined}
        >
          {description ??
            (describing
              ? t("discover.describing")
              : t("discover.noDescription"))}
        </p>
        {state === "added" && skill.added ? (
          <p className="text-caption text-content-muted">
            {t("discover.addedAs", { name: skill.added })}
          </p>
        ) : null}
        {done ? null : (
          <DiscoverAppChoice
            itemName={skill.name}
            targets={targets}
            unreachable={[]}
            selected={selected}
            disabled={submitting}
            onChange={setSelected}
          />
        )}
        {refusalCopy ? (
          <p role="alert" className="text-caption text-danger">
            {t(refusalCopy.messageKey)}
          </p>
        ) : null}
      </div>
    </Modal>
  );
}
