import { ExternalLink, Scale } from "lucide-react";
import { useId } from "react";
import { useTranslation } from "react-i18next";
import { toast } from "sonner";
import { useUpdateStatus } from "@/entities/update";
import { toErrorCopy } from "@/shared/lib/nativeError";
import { Button } from "@/shared/ui/Button";
import { Card } from "@/shared/ui/Card";
import { SectionHeader } from "@/shared/ui/SectionHeader";
import { useOpenLicense, useOpenSourceCode } from "./useLegalNoticeLinks";

/**
 * The Appropriate Legal Notices AGPL-3.0 §5 asks an interactive program to
 * show: who holds the copyright, that there is no warranty, which licence
 * applies, and where the corresponding source can be obtained.
 *
 * It stays a short notice on purpose. The licence text and the repository are
 * links the native side opens in the browser — embedding a licence reader in
 * a settings page would bury the four sentences that actually matter.
 */
export function AboutSection() {
  const { t } = useTranslation();
  const headingId = useId();
  const status = useUpdateStatus();
  const openSourceCode = useOpenSourceCode();
  const openLicense = useOpenLicense();
  const version = status.data?.currentVersion ?? null;

  function reportFailure(error: unknown): void {
    const copy = toErrorCopy(error);
    toast.error(t(copy.messageKey), {
      description: copy.remediationKey ? t(copy.remediationKey) : undefined,
    });
  }

  return (
    <section aria-labelledby={headingId} className="flex flex-col gap-4">
      <SectionHeader
        as="h3"
        headingId={headingId}
        title={t("about.title")}
        description={t("about.description")}
      />
      <Card className="flex flex-col gap-4">
        <div className="flex items-start gap-3">
          <span className="flex h-9 w-9 shrink-0 items-center justify-center rounded-lg bg-layer-1 text-brand">
            <Scale className="h-4 w-4" aria-hidden="true" />
          </span>
          <div className="min-w-0">
            <p className="text-body font-medium text-content">
              {t("about.product")}
            </p>
            <p className="mt-0.5 text-caption text-content-muted">
              {version
                ? t("about.version", { version })
                : t("about.versionUnknown")}
            </p>
          </div>
        </div>

        <div className="flex flex-col gap-1.5 border-t border-hairline pt-4 text-caption leading-5 text-content-muted">
          <p className="text-content">{t("about.copyright")}</p>
          <p>{t("about.license")}</p>
          <p>{t("about.upstream")}</p>
        </div>

        <div className="flex flex-wrap gap-2">
          <Button
            size="sm"
            variant="secondary"
            loading={openSourceCode.isPending}
            onClick={() => {
              openSourceCode.mutate(undefined, { onError: reportFailure });
            }}
          >
            <ExternalLink className="h-4 w-4" aria-hidden="true" />
            {t("about.sourceCode")}
          </Button>
          <Button
            size="sm"
            variant="ghost"
            loading={openLicense.isPending}
            onClick={() => {
              openLicense.mutate(undefined, { onError: reportFailure });
            }}
          >
            <ExternalLink className="h-4 w-4" aria-hidden="true" />
            {t("about.licenseText")}
          </Button>
        </div>
      </Card>
    </section>
  );
}
