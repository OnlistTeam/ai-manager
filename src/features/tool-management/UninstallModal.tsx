import { useEffect, useState } from "react";
import { AlertTriangle, ShieldCheck } from "lucide-react";
import { useTranslation } from "react-i18next";
import type { UninstallOptions } from "@/native";
import {
  type Tool,
  type ToolUninstallTarget,
  useToolUninstallPreview,
} from "@/entities/tool";
import { Button } from "@/shared/ui/Button";
import { Modal } from "@/shared/ui/Modal";
import { TooltipProvider } from "@/shared/ui/Tooltip";
import { ToolActionError } from "./ToolActionError";
import { UninstallOptionsList } from "./UninstallOptionsList";
import { UninstallTargetList } from "./UninstallTargetList";
import { describeUninstall } from "./uninstallSummary";

export interface UninstallModalProps {
  /** `null` = closed. While open, this also serves as the payload for "which tool to remove". */
  tool: Tool | null;
  busy?: boolean;
  error?: Error | null;
  onOpenChange: (open: boolean) => void;
  onConfirm: (options: UninstallOptions) => void;
}

export function UninstallModal({
  tool,
  busy = false,
  error = null,
  onOpenChange,
  onConfirm,
}: UninstallModalProps) {
  const { t } = useTranslation();
  const [options, setOptions] = useState<UninstallOptions>({
    removeSettings: false,
    removeCache: false,
  });
  const [confirming, setConfirming] = useState(false);
  const preview = useToolUninstallPreview(tool?.id ?? null);

  // Reset on tool change / modal close: dangerous checkboxes must never carry over to a different target.
  useEffect(() => {
    setOptions({ removeSettings: false, removeCache: false });
    setConfirming(false);
  }, [tool?.id]);

  const impact = describeUninstall(options, {
    sessionsInsideSettings: tool?.sessionsInsideSettings ?? false,
  });
  const name = tool?.name ?? "";
  const appTargets = preview.data?.app ?? [];
  const settingsTargets = preview.data?.settings ?? [];
  const cacheTargets = preview.data?.cache ?? [];
  const previewReady = preview.isSuccess;
  const appIsSafe =
    previewReady &&
    appTargets.length > 0 &&
    appTargets.every((target) => target.canRemoveAutomatically);
  const selectedGroups: Array<{
    key: "app" | "settings" | "cache";
    targets: readonly ToolUninstallTarget[];
  }> = [
    { key: "app", targets: appTargets },
    ...(options.removeCache
      ? ([{ key: "cache", targets: cacheTargets }] as const)
      : []),
    ...(options.removeSettings
      ? ([{ key: "settings", targets: settingsTargets }] as const)
      : []),
  ];

  return (
    <TooltipProvider delayDuration={200} skipDelayDuration={100}>
      <Modal
        open={tool !== null}
        onOpenChange={onOpenChange}
        dismissible={!busy}
        size="md"
        title={
          confirming
            ? t("tools.uninstall.confirmTitle")
            : t("tools.uninstall.title", { name })
        }
        description={
          confirming
            ? t("tools.uninstall.confirmDescription")
            : t("tools.uninstall.description")
        }
        footer={
          confirming ? (
            <>
              <Button
                variant="secondary"
                disabled={busy}
                onClick={() => setConfirming(false)}
              >
                {t("tools.uninstall.back")}
              </Button>
              <Button
                variant="danger"
                loading={busy}
                onClick={() => onConfirm(options)}
              >
                {t("tools.uninstall.confirm")}
              </Button>
            </>
          ) : (
            <>
              <Button
                variant="secondary"
                disabled={busy}
                onClick={() => onOpenChange(false)}
              >
                {t("ds.action.cancel")}
              </Button>
              <Button
                variant={impact.destructive ? "danger" : "primary"}
                loading={busy || preview.isPending}
                disabled={!appIsSafe}
                onClick={() =>
                  impact.destructive ? setConfirming(true) : onConfirm(options)
                }
              >
                {t("tools.uninstall.submit")}
              </Button>
            </>
          )
        }
      >
        {confirming ? (
          <div className="flex flex-col gap-4">
            <div className="flex flex-col gap-3">
              {selectedGroups.map((group) => (
                <div key={group.key}>
                  <div className="flex items-start gap-2 text-body text-content">
                    <AlertTriangle
                      className="mt-0.5 h-4 w-4 shrink-0 text-danger"
                      aria-hidden="true"
                    />
                    {t(`tools.uninstall.impact.${group.key}`, { name })}
                  </div>
                  <div className="pl-6">
                    <UninstallTargetList targets={group.targets} />
                  </div>
                </div>
              ))}
            </div>
            {impact.warningKeys.length > 0 ? (
              <div className="flex flex-col gap-2 rounded-md bg-danger/10 p-3">
                {impact.warningKeys.map((key) => (
                  <p
                    key={key}
                    className="flex items-start gap-2 text-caption text-content"
                  >
                    <AlertTriangle
                      className="mt-0.5 h-4 w-4 shrink-0 text-danger"
                      aria-hidden="true"
                    />
                    {t(key)}
                  </p>
                ))}
              </div>
            ) : null}
            {error ? <ToolActionError error={error} /> : null}
          </div>
        ) : (
          <div className="flex flex-col gap-4">
            <UninstallOptionsList
              options={options}
              preview={preview.data}
              previewPending={preview.isPending}
              busy={busy}
              onChange={setOptions}
            />

            {impact.destructive ? (
              <div className="flex flex-col gap-2 rounded-md bg-danger/10 p-3">
                {impact.warningKeys.map((key) => (
                  <p
                    key={key}
                    className="flex items-start gap-2 text-caption text-content"
                  >
                    <AlertTriangle
                      className="mt-0.5 h-4 w-4 shrink-0 text-danger"
                      aria-hidden="true"
                    />
                    {t(key)}
                  </p>
                ))}
              </div>
            ) : (
              <div className="flex items-start gap-2 rounded-md bg-success/10 p-3">
                <ShieldCheck
                  className="mt-0.5 h-4 w-4 shrink-0 text-success"
                  aria-hidden="true"
                />
                <p className="text-caption text-content">
                  <span className="block text-body">
                    {t("tools.uninstall.keepSettings")}
                  </span>
                  {t("tools.uninstall.keepSettingsHint")}
                </p>
              </div>
            )}
            {previewReady ? (
              <p className="text-caption text-content-muted">
                {t("tools.uninstall.preview.rechecked")}
              </p>
            ) : null}
            {preview.error ? <ToolActionError error={preview.error} /> : null}
            {error ? <ToolActionError error={error} /> : null}
          </div>
        )}
      </Modal>
    </TooltipProvider>
  );
}
