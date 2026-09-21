import { AlertCircle, Check } from "lucide-react";
import { useTranslation } from "react-i18next";
import type { SkillCatalogItem } from "@/entities/skill-catalog";
import { toErrorCopy } from "@/shared/lib/nativeError";
import { Badge } from "@/shared/ui/Badge";
import { Button } from "@/shared/ui/Button";

export interface SkillCatalogItemRowProps {
  skill: SkillCatalogItem;
  busy: boolean;
  pending: boolean;
  error: Error | null;
  onInstall: (skill: SkillCatalogItem) => void;
}

/** Keeps one catalog action and its recoverable result in the same row. */
export function SkillCatalogItemRow({
  skill,
  busy,
  pending,
  error,
  onInstall,
}: SkillCatalogItemRowProps) {
  const { t } = useTranslation();
  const copy = error ? toErrorCopy(error) : null;
  const errorTitle = t("extensions.skill.catalog.installErrorNamed", {
    name: skill.name,
  });

  return (
    <li className="min-w-0 rounded-lg border border-hairline bg-layer-1 p-4">
      <div className="flex min-w-0 items-start gap-4">
        <div className="min-w-0 flex-1">
          <div className="flex flex-wrap items-center gap-2">
            <h3 className="break-words font-medium text-content">
              {skill.name}
            </h3>
            {skill.installed ? (
              <Badge tone="success" icon={Check}>
                {t("extensions.skill.catalog.installed")}
              </Badge>
            ) : null}
          </div>
          <p className="mt-1 break-words [overflow-wrap:anywhere] text-caption text-content-muted">
            {skill.description || t("extensions.card.noDescription")}
          </p>
          <p className="mt-2 break-words [overflow-wrap:anywhere] text-caption text-content-muted">
            {t("extensions.skill.catalog.source", {
              source: `${skill.source.owner}/${skill.source.repository}`,
            })}
          </p>
        </div>
        <Button
          size="sm"
          variant={skill.installed ? "secondary" : "primary"}
          disabled={skill.installed || busy}
          loading={pending}
          aria-label={t(
            skill.installed
              ? "extensions.skill.catalog.installedNamed"
              : copy
                ? "extensions.skill.catalog.retryInstallNamed"
                : "extensions.skill.catalog.installNamed",
            { name: skill.name },
          )}
          onClick={() => onInstall(skill)}
        >
          {skill.installed
            ? t("extensions.skill.catalog.installed")
            : copy
              ? t("extensions.skill.catalog.retry")
              : t("extensions.skill.catalog.install")}
        </Button>
      </div>

      {copy ? (
        <div
          role="alert"
          aria-label={errorTitle}
          className="mt-3 flex items-start gap-2 rounded-md border border-danger/30 bg-danger/5 p-3"
        >
          <AlertCircle
            className="mt-0.5 h-4 w-4 shrink-0 text-danger"
            aria-hidden="true"
          />
          <div className="min-w-0">
            <p className="text-caption font-medium text-content">
              {errorTitle}
            </p>
            <p className="mt-0.5 text-caption leading-5 text-content-muted">
              {t(copy.messageKey)}
            </p>
            {copy.remediationKey ? (
              <p className="mt-0.5 text-caption leading-5 text-content-muted">
                {t(copy.remediationKey)}
              </p>
            ) : null}
          </div>
        </div>
      ) : null}
    </li>
  );
}
