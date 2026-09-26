import { Settings } from "lucide-react";
import { useTranslation } from "react-i18next";
import { usePrivacyProtection } from "@/entities/privacy-protection";
import { Button } from "@/shared/ui/Button";
import { cn } from "@/shared/ui/cn";

export interface PrivacyStatusLineProps {
  /** Opens the privacy section of the settings page. */
  onOpenSettings?: () => void;
  className?: string;
}

/**
 * One line on the live routing card saying what privacy protection hides
 * (ADR-0049). The choices themselves live on the settings page; this line
 * only reports them and links there.
 */
export function PrivacyStatusLine({
  onOpenSettings,
  className,
}: PrivacyStatusLineProps) {
  const { t } = useTranslation();
  const privacy = usePrivacyProtection();

  if (privacy.isPending) return null;

  const parts = privacy.data
    ? [
        t(
          privacy.data.maskSecrets
            ? "routing.privacy.secretsHidden"
            : "routing.privacy.secretsShown",
        ),
        t(
          privacy.data.maskPersonal
            ? "routing.privacy.personalHidden"
            : "routing.privacy.personalShown",
        ),
        ...(privacy.data.words.length > 0
          ? [t("routing.privacy.words", { count: privacy.data.words.length })]
          : []),
      ]
    : [t("routing.privacy.unavailable")];

  return (
    <div className={cn("flex items-center justify-between gap-3", className)}>
      <p className="min-w-0 text-caption text-content-muted">
        {t("routing.privacy.summary", { parts: parts.join(" · ") })}
      </p>
      {onOpenSettings ? (
        <Button
          variant="ghost"
          size="xs"
          className="shrink-0"
          aria-label={t("routing.privacy.openSettingsLabel")}
          onClick={onOpenSettings}
        >
          <Settings className="h-3.5 w-3.5" aria-hidden="true" />
          {t("routing.privacy.openSettings")}
        </Button>
      ) : null}
    </div>
  );
}
