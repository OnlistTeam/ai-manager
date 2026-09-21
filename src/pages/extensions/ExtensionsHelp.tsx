import { Info } from "lucide-react";
import { useTranslation } from "react-i18next";
import type { ExtensionTab } from "@/features/extension-management";
import { FOCUS_RING } from "@/shared/ui/focusRing";
import { cn } from "@/shared/ui/cn";

/** Keep explanations available without placing several paragraphs above every inventory. */
export function ExtensionsHelp({ tab }: { tab: ExtensionTab }) {
  const { t } = useTranslation();
  return (
    <details className="relative">
      <summary
        tabIndex={0}
        onKeyDown={(event) => {
          if (event.key === "Escape") {
            event.currentTarget.parentElement?.removeAttribute("open");
            event.currentTarget.focus();
          }
        }}
        className={cn(
          "flex cursor-pointer list-none items-center gap-1.5 rounded-md px-2 py-1 text-caption text-content-muted hover:bg-layer-2 hover:text-content [&::-webkit-details-marker]:hidden",
          FOCUS_RING,
        )}
      >
        <Info className="h-4 w-4" aria-hidden="true" />
        {t("extensions.help")}
      </summary>
      <div className="absolute right-0 top-full z-20 mt-2 flex w-64 flex-col gap-2 rounded-xl border border-hairline bg-surface p-4 text-caption text-content-muted shadow-md">
        <p>{t(tab.explainerKey)}</p>
        {tab.noteKey ? <p>{t(tab.noteKey)}</p> : null}
        {tab.kind !== "skill" ? (
          <p>{t("extensions.location.description")}</p>
        ) : null}
        {tab.kind === "prompt" ? (
          <p>{t("extensions.location.promptNote")}</p>
        ) : null}
      </div>
    </details>
  );
}
