import { useTranslation } from "react-i18next";
import type {
  ToolUninstallPreview,
  ToolUninstallTarget,
} from "@/entities/tool";
import type { UninstallOptions } from "@/native";
import { Checkbox } from "@/shared/ui/Checkbox";
import { cn } from "@/shared/ui/cn";
import { Tooltip, TooltipContent, TooltipTrigger } from "@/shared/ui/Tooltip";
import { UninstallTargetList } from "./UninstallTargetList";

const OPTIONAL_ROWS = [
  {
    key: "settings",
    field: "removeSettings",
    labelKey: "tools.uninstall.option.settings",
    hintKey: "tools.uninstall.option.settingsHint",
  },
  {
    key: "cache",
    field: "removeCache",
    labelKey: "tools.uninstall.option.cache",
    hintKey: "tools.uninstall.option.cacheHint",
  },
] as const;

type Unavailable = "nothingToRemove" | "protected" | null;

/**
 * Why an option cannot be picked, or `null` when it can.
 *
 * A checkbox that will not tick reads as a broken control unless the reason is
 * right there, and the two reasons are not the same: one means there is nothing
 * on disk to delete, the other means there is, but it lies outside what the app
 * is willing to touch.
 */
function unavailableReason(
  preview: ToolUninstallPreview | undefined,
  targets: readonly ToolUninstallTarget[],
): Unavailable {
  if (preview === undefined || targets.length === 0) return "nothingToRemove";
  if (targets.some((target) => !target.canRemoveAutomatically))
    return "protected";
  return null;
}

export interface UninstallOptionsListProps {
  options: UninstallOptions;
  preview?: ToolUninstallPreview;
  previewPending: boolean;
  busy: boolean;
  onChange: (options: UninstallOptions) => void;
}

export function UninstallOptionsList({
  options,
  preview,
  previewPending,
  busy,
  onChange,
}: UninstallOptionsListProps) {
  const { t } = useTranslation();

  return (
    <div className="flex flex-col gap-3">
      <div className="flex items-start gap-3 text-body text-content">
        <Checkbox
          id="uninstall-app"
          checked
          disabled
          onCheckedChange={() => {}}
          aria-describedby="uninstall-app-hint"
        />
        <div className="min-w-0">
          <label htmlFor="uninstall-app" className="cursor-not-allowed">
            {t("tools.uninstall.option.app")}
          </label>
          <p
            id="uninstall-app-hint"
            className="mt-0.5 text-caption text-content-muted"
          >
            {t("tools.uninstall.option.appHint")}
          </p>
          <UninstallTargetList
            targets={preview?.app ?? []}
            loading={previewPending}
          />
        </div>
      </div>

      {OPTIONAL_ROWS.map((row) => {
        const targets = preview?.[row.key] ?? [];
        const reason = unavailableReason(preview, targets);
        const inputId = `uninstall-${row.key}`;
        const hintId = `${inputId}-hint`;
        const reasonId = `${inputId}-reason`;
        const blocked = !previewPending && reason !== null;
        const reasonText = reason ? t(`tools.uninstall.reason.${reason}`) : "";
        const checkbox = (
          <Checkbox
            id={inputId}
            checked={options[row.field]}
            disabled={busy || reason !== null}
            onCheckedChange={(checked) =>
              onChange({ ...options, [row.field]: checked })
            }
            aria-describedby={
              blocked && reason === "nothingToRemove"
                ? `${hintId} ${reasonId}`
                : hintId
            }
          />
        );
        return (
          <div
            key={row.key}
            className="flex items-start gap-3 text-body text-content"
          >
            {blocked ? (
              <Tooltip>
                {/* A disabled control fires no pointer events, so the trigger
                    wraps it instead of being it. */}
                <TooltipTrigger asChild>
                  <span className="flex">{checkbox}</span>
                </TooltipTrigger>
                <TooltipContent side="right">{reasonText}</TooltipContent>
              </Tooltip>
            ) : (
              checkbox
            )}
            <div className="min-w-0">
              <label
                htmlFor={inputId}
                className={cn(
                  blocked ? "cursor-not-allowed" : "cursor-pointer",
                )}
              >
                {t(row.labelKey)}
              </label>
              <p id={hintId} className="mt-0.5 text-caption text-content-muted">
                {t(row.hintKey)}
              </p>
              {/* Nothing to list means the sentence is the whole story; a
                  protected target still shows its paths, each of which carries
                  its own warning, so the sentence would only repeat them. */}
              {blocked && reason === "nothingToRemove" ? (
                <p
                  id={reasonId}
                  className="mt-1 text-caption text-content-muted"
                >
                  {reasonText}
                </p>
              ) : (
                <UninstallTargetList
                  targets={targets}
                  loading={previewPending}
                />
              )}
            </div>
          </div>
        );
      })}
    </div>
  );
}
