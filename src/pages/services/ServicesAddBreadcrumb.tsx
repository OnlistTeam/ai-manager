import { ChevronRight } from "lucide-react";
import { useTranslation } from "react-i18next";
import { cn } from "@/shared/ui/cn";
import { FOCUS_RING } from "@/shared/ui/focusRing";

export interface ServicesAddBreadcrumbProps {
  toolName: string;
  onBack: () => void;
}

/** Where the tabs were while the add page is open: the way back, and where you are. */
export function ServicesAddBreadcrumb({
  toolName,
  onBack,
}: ServicesAddBreadcrumbProps) {
  const { t } = useTranslation();
  return (
    <nav aria-label={t("services.add.breadcrumb")}>
      <ol className="flex min-w-0 flex-wrap items-center gap-1.5 text-body">
        <li>
          <button
            type="button"
            onClick={onBack}
            className={cn(
              "rounded-sm text-content-muted transition-colors hover:text-content",
              FOCUS_RING,
            )}
          >
            {t("services.add.back")}
          </button>
        </li>
        <li aria-hidden="true" className="text-content-muted">
          <ChevronRight className="h-4 w-4" />
        </li>
        <li className="min-w-0">
          <h2
            aria-current="page"
            className="truncate text-heading text-content"
          >
            {t("services.add.title", { tool: toolName })}
          </h2>
        </li>
      </ol>
    </nav>
  );
}
