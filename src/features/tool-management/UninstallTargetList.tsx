import { useTranslation } from "react-i18next";
import type { ToolUninstallTarget } from "@/entities/tool";
import { RevealPathButton } from "./RevealPathButton";

export interface UninstallTargetListProps {
  targets: readonly ToolUninstallTarget[];
  loading?: boolean;
}

/** A command has no location to open; everything else is somewhere on disk. */
function isOnDisk(target: ToolUninstallTarget): boolean {
  return target.kind !== "command";
}

export function UninstallTargetList({
  targets,
  loading = false,
}: UninstallTargetListProps) {
  const { t } = useTranslation();

  if (loading) {
    return (
      <p className="mt-1.5 text-caption text-content-muted" role="status">
        {t("tools.uninstall.preview.loading")}
      </p>
    );
  }

  if (targets.length === 0) {
    return (
      <p className="mt-1.5 text-caption text-content-muted">
        {t("tools.uninstall.preview.none")}
      </p>
    );
  }

  return (
    <ul className="mt-2 flex flex-col gap-1.5">
      {targets.map((target) => (
        <li
          key={`${target.kind}:${target.value}`}
          className="rounded-sm border border-hairline bg-layer-1 py-1.5 pl-2.5 pr-1.5"
        >
          <div className="flex min-w-0 items-start gap-2">
            <span className="mt-1 shrink-0 text-caption text-content-muted">
              {t(`tools.uninstall.preview.kind.${target.kind}`)}
            </span>
            <code
              data-selectable-text=""
              className="mt-1 min-w-0 flex-1 break-all font-mono text-caption text-content"
            >
              {target.value}
            </code>
            {isOnDisk(target) ? <RevealPathButton path={target.value} /> : null}
          </div>
          {!target.canRemoveAutomatically ? (
            <p className="mt-1 pr-1 text-caption text-danger">
              {t("tools.uninstall.preview.protected")}
            </p>
          ) : null}
        </li>
      ))}
    </ul>
  );
}
