import { FolderOpen } from "lucide-react";
import { toast } from "sonner";
import { useTranslation } from "react-i18next";
import { native } from "@/native";
import { toErrorCopy } from "@/shared/lib/nativeError";
import { cn } from "@/shared/ui/cn";
import { FOCUS_RING } from "@/shared/ui/focusRing";
import { Tooltip, TooltipContent, TooltipTrigger } from "@/shared/ui/Tooltip";

export interface RevealPathButtonProps {
  path: string;
  className?: string;
}

/**
 * Opens a path in Finder / Explorer from next to the path itself.
 *
 * A dialog that is about to delete a folder should let you go look at it first.
 * The native side survives the path having already gone away, so this stays
 * available for targets the preview reports as missing.
 */
export function RevealPathButton({ path, className }: RevealPathButtonProps) {
  const { t } = useTranslation();
  const label = t("tools.uninstall.preview.reveal");

  return (
    <Tooltip>
      <TooltipTrigger asChild>
        <button
          type="button"
          aria-label={t("tools.uninstall.preview.revealNamed", { path })}
          onClick={() => {
            void native.system.revealPath(path).catch((error: unknown) => {
              const copy = toErrorCopy(error);
              toast.error(t(copy.messageKey), {
                description: copy.remediationKey
                  ? t(copy.remediationKey)
                  : undefined,
              });
            });
          }}
          className={cn(
            "flex h-7 w-7 shrink-0 items-center justify-center rounded-sm text-content-muted",
            "transition-colors duration-fast ease-standard hover:bg-layer-2 hover:text-content",
            FOCUS_RING,
            className,
          )}
        >
          <FolderOpen className="h-3.5 w-3.5" aria-hidden="true" />
        </button>
      </TooltipTrigger>
      <TooltipContent side="left">{label}</TooltipContent>
    </Tooltip>
  );
}
