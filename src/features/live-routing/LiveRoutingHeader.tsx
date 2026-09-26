import { AtSign } from "lucide-react";
import { useTranslation } from "react-i18next";
import { Button } from "@/shared/ui/Button";

/** "Routing · ● Live", with the email masking toggle on the right. */
export function LiveRoutingHeader({
  hideEmails,
  onHideEmailsChange,
}: {
  hideEmails: boolean;
  onHideEmailsChange: (hide: boolean) => void;
}) {
  const { t } = useTranslation();
  const label = t("routing.live.header.hideEmails");

  return (
    <div className="flex items-center justify-between gap-3 border-b border-hairline px-3 py-2">
      <h3 className="flex min-w-0 items-center gap-1.5 text-caption font-medium text-content">
        <span>{t("routing.live.header.title")}</span>
        <span aria-hidden="true" className="text-content-muted">
          ·
        </span>
        <span className="flex items-center gap-1 text-content-muted">
          <span
            aria-hidden="true"
            className="h-1.5 w-1.5 rounded-full bg-success motion-safe:animate-pulse"
          />
          {t("routing.live.header.live")}
        </span>
      </h3>
      <Button
        size="xs"
        variant={hideEmails ? "secondary" : "ghost"}
        aria-pressed={hideEmails}
        title={label}
        onClick={() => onHideEmailsChange(!hideEmails)}
      >
        <AtSign className="h-3.5 w-3.5" aria-hidden="true" />
        {label}
      </Button>
    </div>
  );
}
