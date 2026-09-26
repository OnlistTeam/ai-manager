import { Plus, Trash2 } from "lucide-react";
import { useState } from "react";
import { useTranslation } from "react-i18next";
import type { Provider, ProviderEditProfile } from "@/entities/provider";
import { Button } from "@/shared/ui/Button";
import { Field } from "@/shared/ui/Field";
import { CopyableInput } from "@/shared/ui/CopyableInput";
import { ProviderEndpointFields } from "./ProviderEndpointFields";
import { ProviderHeaderFields } from "./ProviderHeaderFields";
import { ProviderModelInput } from "./ProviderModelInput";
import { trailingVersionSegment } from "./providerEndpointRouteUtils";
import { useModelCatalog } from "./useProviderModelProbe";
import type { useProviderSettingsForm } from "./useProviderSettingsForm";

type SettingsForm = ReturnType<typeof useProviderSettingsForm>;

interface ProviderSettingsFieldsProps {
  provider: Provider;
  profile: ProviderEditProfile | null;
  /** The key box no longer holds the saved key. */
  keyChanged: boolean;
  loading: boolean;
  failed: boolean;
  disabled: boolean;
  form: SettingsForm;
  onChange: () => void;
}

export function ProviderSettingsFields({
  provider,
  profile,
  keyChanged,
  loading,
  failed,
  disabled,
  form,
  onChange,
}: ProviderSettingsFieldsProps) {
  const { t } = useTranslation();
  // The list is read only once the user opens it (ADR-0041): it is a request
  // to the service carrying the saved key.
  const [catalogRequested, setCatalogRequested] = useState(false);
  const catalogQuery = useModelCatalog(
    catalogRequested ? { kind: "provider", provider } : null,
  );

  if (loading) {
    return (
      <p role="status" className="text-caption text-content-muted">
        {t("services.form.settingsLoading")}
      </p>
    );
  }
  if (failed || profile === null) {
    return (
      <p className="rounded-lg bg-layer-1 p-3 text-caption text-content-muted">
        {t("services.form.settingsUnavailable")}
      </p>
    );
  }

  const capabilities = profile.capabilities;
  const catalog = {
    catalog: catalogQuery.data,
    loading: catalogQuery.isFetching && catalogQuery.data === undefined,
    error: catalogQuery.error,
    stale: keyChanged || form.baseUrl.trim() !== (profile.baseUrl ?? ""),
    onRequest: () => setCatalogRequested(true),
    onRetry: () => void catalogQuery.refetch(),
  };
  const versionSegment = profile.baseUrlTakesNoVersion
    ? trailingVersionSegment(form.baseUrl)
    : null;
  const baseUrlWarning =
    versionSegment === null
      ? undefined
      : t("services.form.baseUrlVersionDoubled", { segment: versionSegment });
  return (
    <>
      <section className="flex flex-col gap-4 border-t border-hairline pt-4">
        {capabilities.canEditEndpoints ? (
          <ProviderEndpointFields
            disabled={disabled}
            takesNoVersion={profile.baseUrlTakesNoVersion}
            form={form.endpoints}
            onChange={onChange}
          />
        ) : capabilities.canEditBaseUrl ? (
          <Field
            id="service-base-url"
            label={t("services.form.baseUrl")}
            hint={baseUrlWarning ? undefined : t("services.form.baseUrlHint")}
            warning={baseUrlWarning}
          >
            <CopyableInput
              id="service-base-url"
              copyLabel={t("services.form.baseUrl")}
              type="url"
              value={form.baseUrl}
              maxLength={2_048}
              spellCheck={false}
              disabled={disabled}
              className="font-mono"
              onChange={(event) => {
                form.setBaseUrl(event.target.value);
                onChange();
              }}
            />
          </Field>
        ) : (
          <p className="rounded-lg bg-layer-1 p-3 text-caption text-content-muted">
            {t("services.form.advancedManaged")}
          </p>
        )}

        {capabilities.canEditModels ? (
          <section
            className="flex flex-col gap-2"
            aria-label={t("services.form.models")}
          >
            <p className="text-caption text-content">
              {t("services.form.models")}
            </p>
            {form.models.map((row, index) => (
              <div key={row.id} className="flex min-w-0 items-start gap-2">
                <ProviderModelInput
                  id={`service-model-${index}`}
                  label={t("services.form.modelNamed", { index: index + 1 })}
                  value={row.value}
                  disabled={
                    disabled ||
                    (capabilities.supportsMultipleModels && row.existing)
                  }
                  invalid={
                    capabilities.supportsMultipleModels &&
                    row.value.trim() === ""
                  }
                  catalog={catalog}
                  onChange={(value) => {
                    form.updateModel(row.id, value);
                    onChange();
                  }}
                />
                {capabilities.supportsMultipleModels ? (
                  <Button
                    variant="ghost"
                    size="sm"
                    disabled={disabled}
                    aria-label={t("services.form.removeModelNamed", {
                      model: row.value || index + 1,
                    })}
                    onClick={() => {
                      form.removeModel(row.id);
                      onChange();
                    }}
                  >
                    <Trash2 className="h-4 w-4" aria-hidden="true" />
                  </Button>
                ) : null}
              </div>
            ))}
            <p className="text-caption text-content-muted">
              {t(
                capabilities.supportsMultipleModels
                  ? "services.form.modelsHintMultiple"
                  : "services.form.modelsHintSingle",
              )}
            </p>
            {capabilities.supportsMultipleModels ? (
              <Button
                variant="secondary"
                size="sm"
                className="self-start"
                disabled={disabled}
                onClick={() => {
                  form.addModel();
                  onChange();
                }}
              >
                <Plus className="h-4 w-4" aria-hidden="true" />
                {t("services.form.addModel")}
              </Button>
            ) : null}
            {form.invalidModels ? (
              <p role="alert" className="text-caption text-danger">
                {t("services.form.modelRequired")}
              </p>
            ) : null}
          </section>
        ) : null}

        {capabilities.canEditHeaders ? (
          <details className="group rounded-xl border border-hairline bg-layer-1 p-3">
            <summary className="cursor-pointer list-none text-caption font-medium text-content">
              {t("services.form.headers")}
              <span className="ml-2 font-normal text-content-muted">
                {t("services.form.headersHint")}
              </span>
            </summary>
            <div className="mt-3 border-t border-hairline pt-3">
              <ProviderHeaderFields
                disabled={disabled}
                form={form}
                onChange={onChange}
              />
            </div>
          </details>
        ) : null}
      </section>
    </>
  );
}
