import { RotateCw } from "lucide-react";
import { useTranslation } from "react-i18next";
import { toast } from "sonner";
import { useAnyToolRouted } from "@/entities/routing";
import { useInstallAppUpdate, useUpdateStatus } from "@/entities/update";
import { toErrorCopy } from "@/shared/lib/nativeError";
import { Button } from "@/shared/ui/Button";
import {
  Tooltip,
  TooltipContent,
  TooltipProvider,
  TooltipTrigger,
} from "@/shared/ui/Tooltip";

/**
 * A deliberately quiet global update affordance.
 *
 * Only one state reaches the window chrome: an update that is downloaded,
 * verified, and waiting for a restart. That is the only state with something
 * for the user to do.
 *
 * The download itself is deliberately silent. It used to show a progress pill
 * here, which read as the computer fetching something unasked — a fair
 * reading, since nobody requested it and the pill could not say what it was.
 * Progress is still available to anyone who goes looking for it, in Settings,
 * where the question was asked.
 */
export function GlobalUpdateAction() {
  const status = useUpdateStatus();
  if (status.data?.phase !== "ready") return null;
  return <RestartToUpdateButton />;
}

/**
 * The restart puts every routed tool back on its own settings, as quitting
 * does (ADR-0054). While any tool is routed the button says so before it is
 * pressed; there is no second question after it.
 */
function RestartToUpdateButton() {
  const { t } = useTranslation();
  const install = useInstallAppUpdate();
  const anyRouted = useAnyToolRouted();

  const button = (
    <Button
      size="sm"
      variant="secondary"
      loading={install.isPending}
      className="app-update-ready rounded-full border-success/25 bg-success/10 px-4 text-content shadow-[inset_0_1px_0_hsl(var(--content)/0.08),0_10px_26px_hsl(var(--success)/0.08)] hover:border-success/40 hover:bg-success/15"
      onClick={() => {
        install.mutate(undefined, {
          onError: (error) => {
            const copy = toErrorCopy(error);
            toast.error(t(copy.messageKey), {
              description: copy.remediationKey
                ? t(copy.remediationKey)
                : undefined,
            });
          },
        });
      }}
    >
      <RotateCw className="h-3.5 w-3.5 text-success" aria-hidden="true" />
      {t("preferences.updates.restart")}
    </Button>
  );

  if (!anyRouted) return button;
  return (
    <TooltipProvider delayDuration={200}>
      <Tooltip>
        <TooltipTrigger asChild>{button}</TooltipTrigger>
        <TooltipContent side="bottom">
          {t("preferences.updates.routedNote")}
        </TooltipContent>
      </Tooltip>
    </TooltipProvider>
  );
}
