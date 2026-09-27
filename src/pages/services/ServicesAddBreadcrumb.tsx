import { ChevronLeft, ChevronRight } from "lucide-react";
import { useTranslation } from "react-i18next";
import { Button } from "@/shared/ui/Button";

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
          {/* A button, not a muted word: it is the only way back on this page. */}
          <Button
            variant="secondary"
            size="sm"
            className="pl-2"
            onClick={onBack}
          >
            <ChevronLeft className="h-4 w-4" aria-hidden="true" />
            {t("services.add.back")}
          </Button>
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
