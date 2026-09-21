import { FolderOpen } from "lucide-react";
import { useTranslation } from "react-i18next";
import { toast } from "sonner";
import { useRevealSessionFolder } from "@/entities/session";
import { toErrorCopy } from "@/shared/lib/nativeError";
import { cn } from "@/shared/ui/cn";
import { Button } from "@/shared/ui/Button";
import { FOCUS_RING } from "@/shared/ui/focusRing";
import { Tooltip, TooltipContent, TooltipTrigger } from "@/shared/ui/Tooltip";

export interface SessionRevealFolderButtonProps {
  reference: string;
  title: string | null;
  showLabel?: boolean;
}

/**
 * Opens the folder containing a session's source file. Unlike
 * `RevealPathButton`, it never sees the path itself — the opaque
 * `reference` is all that crosses the native boundary, and the native side
 * resolves and reveals the path without handing it back to the renderer.
 */
export function SessionRevealFolderButton({
  reference,
  title,
  showLabel = false,
}: SessionRevealFolderButtonProps) {
  const { t } = useTranslation();
  const reveal = useRevealSessionFolder({
    onError: (error) => {
      const copy = toErrorCopy(error);
      toast.error(t(copy.messageKey), {
        description: copy.remediationKey ? t(copy.remediationKey) : undefined,
      });
    },
  });
  const label = t("sessions.reveal.action");

  if (showLabel)
    return (
      <Button
        variant="ghost"
        size="sm"
        loading={reveal.isPending}
        aria-label={t("sessions.reveal.actionNamed", {
          title: title ?? t("sessions.list.untitled"),
        })}
        onClick={() => reveal.mutate(reference)}
      >
        <FolderOpen className="h-4 w-4" aria-hidden="true" />
        {label}
      </Button>
    );

  return (
    <Tooltip>
      <TooltipTrigger asChild>
        <button
          type="button"
          disabled={reveal.isPending}
          aria-label={t("sessions.reveal.actionNamed", {
            title: title ?? t("sessions.list.untitled"),
          })}
          onClick={() => reveal.mutate(reference)}
          className={cn(
            "flex h-7 w-7 shrink-0 items-center justify-center rounded-sm text-content-muted",
            "transition-colors duration-fast ease-standard hover:bg-layer-2 hover:text-content",
            FOCUS_RING,
          )}
        >
          <FolderOpen className="h-3.5 w-3.5" aria-hidden="true" />
        </button>
      </TooltipTrigger>
      <TooltipContent side="left">{label}</TooltipContent>
    </Tooltip>
  );
}
