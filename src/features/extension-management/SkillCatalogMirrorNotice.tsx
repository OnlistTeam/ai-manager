import { CloudCog } from "lucide-react";
import { useTranslation } from "react-i18next";

/** Post-action evidence: the repository identity stayed GitHub-owned while transport recovered. */
export function SkillCatalogMirrorNotice() {
  const { t } = useTranslation();
  const title = t("extensions.skill.catalog.mirrorTitle");

  return (
    <aside
      role="status"
      aria-label={title}
      className="mt-4 flex items-start gap-3 rounded-lg border border-brand/20 bg-brand/5 px-4 py-3"
    >
      <CloudCog
        className="mt-0.5 h-4 w-4 shrink-0 text-brand"
        aria-hidden="true"
      />
      <div className="min-w-0">
        <p className="text-body font-medium text-content">{title}</p>
        <p className="mt-0.5 text-caption leading-5 text-content-muted">
          {t("extensions.skill.catalog.mirrorDescription")}
        </p>
      </div>
    </aside>
  );
}
