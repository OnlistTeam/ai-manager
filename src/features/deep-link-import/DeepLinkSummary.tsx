import { AlertTriangle, Globe, Link2, ShieldAlert } from "lucide-react";
import { useTranslation } from "react-i18next";
import type { DeepLinkPreview } from "@/entities/deeplink";

export interface DeepLinkSummaryProps {
  preview: DeepLinkPreview;
}

/**
 * What the link will actually change. Every value here came through the native
 * safe projection: a credential is reported as the field that holds it, never
 * as its value, and an application the product does not manage is described
 * rather than echoed.
 */
export function DeepLinkSummary({ preview }: DeepLinkSummaryProps) {
  const { t } = useTranslation();
  const rows: { key: string; label: string; value: string }[] = [
    {
      key: "resource",
      label: t("deeplink.summary.resource"),
      value: t(`deeplink.resource.${preview.resource}`),
    },
    {
      key: "targets",
      label: t("deeplink.summary.targets"),
      value: preview.targets
        .map((target) =>
          target.name && target.supported
            ? target.name
            : t("deeplink.summary.unmanagedTarget"),
        )
        .join(t("deeplink.summary.separator")),
    },
  ];

  if (preview.name) {
    rows.splice(1, 0, {
      key: "name",
      label: t("deeplink.summary.name"),
      value: preview.name,
    });
  }
  if (preview.items.length > 0) {
    rows.push({
      key: "items",
      label: t("deeplink.summary.items"),
      value: preview.items.join(t("deeplink.summary.separator")),
    });
  }

  return (
    <div className="flex flex-col gap-3">
      <p className="flex items-start gap-2 text-caption leading-5 text-content-muted">
        <Link2 className="mt-0.5 h-3.5 w-3.5 shrink-0" aria-hidden="true" />
        {t(`deeplink.origin.${preview.origin}`)}
      </p>

      <dl
        aria-label={t("deeplink.summary.label")}
        className="flex flex-col gap-2 rounded-lg bg-layer-1 p-3"
      >
        {rows.map((row) => (
          <div key={row.key} className="flex flex-wrap gap-x-3 gap-y-0.5">
            <dt className="text-caption text-content-muted">{row.label}</dt>
            <dd className="min-w-0 break-words text-caption text-content">
              {row.value}
            </dd>
          </div>
        ))}
      </dl>

      {preview.endpoint ? (
        <p className="flex items-start gap-2 text-caption leading-5 text-content-muted">
          <Globe className="mt-0.5 h-3.5 w-3.5 shrink-0" aria-hidden="true" />
          <span className="min-w-0 break-all">{preview.endpoint}</span>
        </p>
      ) : null}

      {preview.credentialFields.length > 0 ? (
        <p className="flex items-start gap-2 rounded-lg bg-warning/10 p-3 text-caption leading-5 text-content">
          <ShieldAlert
            className="mt-0.5 h-4 w-4 shrink-0 text-warning"
            aria-hidden="true"
          />
          <span>
            {preview.credentialFields
              .map((field) => t(`deeplink.credential.${field}`))
              .join(t("deeplink.summary.separator"))}
          </span>
        </p>
      ) : null}

      {preview.blocked ? (
        <p
          role="alert"
          className="flex items-start gap-2 rounded-lg bg-danger/10 p-3 text-caption leading-5 text-content"
        >
          <AlertTriangle
            className="mt-0.5 h-4 w-4 shrink-0 text-danger"
            aria-hidden="true"
          />
          <span>{t(`deeplink.blocked.${preview.blocked}`)}</span>
        </p>
      ) : null}
    </div>
  );
}
