import { Archive, RefreshCw, ShieldCheck } from "lucide-react";
import { useEffect, useRef } from "react";
import { useTranslation } from "react-i18next";
import { toErrorCopy } from "@/shared/lib/nativeError";
import { Button } from "@/shared/ui/Button";

interface SkillZipInstallPanelProps {
  blocked: boolean;
  pending: boolean;
  error: Error | null;
  onChoose: () => void;
}

export function SkillZipInstallPanel({
  blocked,
  pending,
  error,
  onChoose,
}: SkillZipInstallPanelProps) {
  const { t } = useTranslation();
  const chooseRef = useRef<HTMLButtonElement>(null);
  const errorCopy = error ? toErrorCopy(error) : null;

  useEffect(() => {
    chooseRef.current?.focus();
  }, []);

  return (
    <div className="flex flex-col gap-4">
      <section className="rounded-xl border border-hairline bg-layer-1 p-4">
        <div className="flex items-start gap-3">
          <span className="flex h-10 w-10 shrink-0 items-center justify-center rounded-xl bg-brand/10 text-brand">
            <ShieldCheck className="h-5 w-5" aria-hidden="true" />
          </span>
          <div className="min-w-0">
            <h3 className="font-medium text-content">
              {t("extensions.skill.zip.trustTitle")}
            </h3>
            <p className="mt-1 text-caption leading-5 text-content-muted">
              {t("extensions.skill.zip.trustDescription")}
            </p>
          </div>
        </div>
      </section>

      <div className="grid gap-3 sm:grid-cols-2">
        <section className="rounded-xl border border-hairline bg-layer-1 px-4 py-3.5">
          <div className="flex items-center gap-2 text-content">
            <Archive className="h-4 w-4 text-brand" aria-hidden="true" />
            <h3 className="font-medium">
              {t("extensions.skill.zip.formatTitle")}
            </h3>
          </div>
          <p className="mt-2 text-caption leading-5 text-content-muted">
            {t("extensions.skill.zip.formatDescription")}
          </p>
        </section>
        <section className="rounded-xl border border-hairline bg-layer-1 px-4 py-3.5">
          <div className="flex items-center gap-2 text-content">
            <RefreshCw className="h-4 w-4 text-brand" aria-hidden="true" />
            <h3 className="font-medium">
              {t("extensions.skill.zip.syncTitle")}
            </h3>
          </div>
          <p className="mt-2 text-caption leading-5 text-content-muted">
            {t("extensions.skill.zip.syncDescription")}
          </p>
        </section>
      </div>

      <p className="rounded-lg border border-warning/25 bg-warning/10 px-3 py-2.5 text-caption leading-5 text-content-muted">
        {t("extensions.skill.zip.partialNote")}
      </p>

      {errorCopy ? (
        <div
          role="alert"
          aria-label={t(errorCopy.messageKey)}
          className="rounded-xl border border-danger/30 bg-danger/5 px-4 py-3"
        >
          <p className="text-caption font-medium text-content">
            {t(errorCopy.messageKey)}
          </p>
          <p className="mt-1 text-caption text-content-muted">
            {t("extensions.skill.zip.errorHint")}
          </p>
        </div>
      ) : null}

      <div className="flex justify-end">
        <Button
          ref={chooseRef}
          loading={pending}
          disabled={blocked}
          onClick={onChoose}
        >
          <Archive className="h-4 w-4" aria-hidden="true" />
          {t("extensions.skill.zip.choose")}
        </Button>
      </div>
    </div>
  );
}
