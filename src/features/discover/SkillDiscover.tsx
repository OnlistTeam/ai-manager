import { useMemo, useState } from "react";
import { BadgeCheck, Download } from "lucide-react";
import { useTranslation } from "react-i18next";
import {
  useDiscoverSkillDescriptions,
  useDiscoverSkills,
  type DiscoverSkill,
} from "@/entities/discover";
import type { ExtensionScopeOption } from "@/features/extension-management";
import type { ToolId } from "@/native";
import { toErrorCopy } from "@/shared/lib/nativeError";
import { DiscoverAddControl } from "./DiscoverAddControl";
import { DiscoverCard } from "./DiscoverCard";
import { DiscoverFrame } from "./DiscoverFrame";
import { DiscoverLogo } from "./DiscoverLogo";
import { compactCount } from "./discoverLabels";
import { SkillDiscoverSheet } from "./SkillDiscoverSheet";
import { useDiscoverAdds } from "./useDiscoverAdds";
import { useInstallDiscoveredSkill } from "./useDiscoverMutations";
import type { DiscoverSearch } from "./useDiscoverSearch";

export interface SkillDiscoverProps {
  headingId: string;
  search: DiscoverSearch;
  visible: boolean;
  targets: readonly ExtensionScopeOption[];
  blocked: boolean;
}

function toolsOf(targets: readonly ExtensionScopeOption[]): ToolId[] {
  return targets.flatMap((target) =>
    target.scope.kind === "tool" ? [target.scope.id] : [],
  );
}

export function SkillDiscover({
  headingId,
  search,
  visible,
  targets,
  blocked,
}: SkillDiscoverProps) {
  const { t, i18n } = useTranslation();
  const list = useDiscoverSkills(search.query, visible);
  const adds = useDiscoverAdds();
  const install = useInstallDiscoveredSkill();
  const [opened, setOpened] = useState<DiscoverSkill | null>(null);
  const items = useMemo(() => list.data?.items ?? [], [list.data]);
  const undescribed = useMemo(
    () =>
      items.filter((item) => item.description === null).map((item) => item.id),
    [items],
  );
  const described = useDiscoverSkillDescriptions(undescribed);
  const descriptionOf = (skill: DiscoverSkill) =>
    skill.description ?? described.data?.[skill.id] ?? null;
  const describing = described.isFetching;

  const add = (
    skill: DiscoverSkill,
    chosen: readonly ExtensionScopeOption[],
    quiet: boolean,
  ) =>
    adds.start(
      skill.id,
      skill.name,
      () => install.mutateAsync({ skill: skill.id, tools: toolsOf(chosen) }),
      quiet,
    );

  const sourceError = list.data?.sourceError
    ? t(list.data.sourceError.messageKey)
    : null;
  const failure = adds.failure
    ? t("discover.addFailed", {
        name: adds.failure.name,
        reason: t(toErrorCopy(adds.failure.error).messageKey),
      })
    : null;
  const listError = list.isError ? t(toErrorCopy(list.error).messageKey) : null;
  const opening = opened
    ? (items.find((item) => item.id === opened.id) ?? opened)
    : null;

  return (
    <>
      <DiscoverFrame
        headingId={headingId}
        note={
          search.query && list.data
            ? t("discover.results", { count: items.length })
            : t("discover.source.skill")
        }
        search={search}
        searchLabel={t("discover.search.skill")}
        placeholder={t("discover.search.skillPlaceholder")}
        loading={!list.data && !list.isError}
        refreshing={list.isFetching && Boolean(list.data)}
        message={failure ?? listError ?? sourceError}
        empty={
          list.data && items.length === 0
            ? search.query
              ? t("discover.noMatch", { query: search.query })
              : t("discover.empty")
            : null
        }
      >
        {items.map((skill) => (
          <DiscoverCard
            key={skill.id}
            title={skill.name}
            detailsLabel={t("discover.details", { name: skill.name })}
            logo={<DiscoverLogo url={skill.icon} name={skill.source} />}
            titleExtra={
              skill.official ? (
                <BadgeCheck
                  role="img"
                  aria-label={t("discover.officialHint")}
                  className="h-3.5 w-3.5 shrink-0 text-brand"
                />
              ) : null
            }
            meta={<span className="truncate">{skill.source}</span>}
            description={descriptionOf(skill)}
            describing={describing}
            footer={
              <span
                className="inline-flex items-center gap-1 tabular-nums"
                title={t("discover.installs", {
                  value: skill.installs.toLocaleString(i18n.language),
                })}
              >
                <Download className="h-3 w-3" aria-hidden="true" />
                {t("discover.installs", {
                  value: compactCount(skill.installs, i18n.language),
                })}
              </span>
            }
            action={
              <DiscoverAddControl
                name={skill.name}
                state={adds.stateOf(skill.id, skill.added !== null)}
                addedAs={skill.added}
                needsInput={false}
                disabled={blocked || targets.length === 0}
                onAdd={() => void add(skill, targets, false)}
              />
            }
            onOpen={() => setOpened(skill)}
          />
        ))}
      </DiscoverFrame>
      {opening ? (
        <SkillDiscoverSheet
          key={opening.id}
          skill={opening}
          description={descriptionOf(opening)}
          describing={describing}
          targets={targets}
          state={adds.stateOf(opening.id, opening.added !== null)}
          blocked={blocked}
          onClose={() => setOpened(null)}
          onAdd={(chosen) => add(opening, chosen, true)}
        />
      ) : null}
    </>
  );
}
