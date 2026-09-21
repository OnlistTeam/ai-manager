import { Plus, Trash2 } from "lucide-react";
import { useTranslation } from "react-i18next";
import { Button } from "@/shared/ui/Button";
import { Input } from "@/shared/ui/Input";
import type { useProviderSettingsForm } from "./useProviderSettingsForm";

interface ProviderHeaderFieldsProps {
  disabled: boolean;
  form: ReturnType<typeof useProviderSettingsForm>;
  onChange: () => void;
}

export function ProviderHeaderFields({
  disabled,
  form,
  onChange,
}: ProviderHeaderFieldsProps) {
  const { t } = useTranslation();
  return (
    <section
      className="flex flex-col gap-2"
      aria-label={t("services.form.headers")}
    >
      <div>
        <p className="text-caption text-content">
          {t("services.form.headers")}
        </p>
        <p className="text-caption text-content-muted">
          {t("services.form.headersHint")}
        </p>
      </div>
      {form.headers.map((row, index) => (
        <div
          key={row.id}
          className="grid min-w-0 grid-cols-[minmax(0,1fr)_minmax(0,1fr)_auto] gap-2"
        >
          <Input
            aria-label={t("services.form.headerNameNamed", {
              index: index + 1,
            })}
            value={row.name}
            maxLength={128}
            spellCheck={false}
            disabled={disabled || row.existing}
            invalid={row.name.trim() === ""}
            className="min-w-0 font-mono"
            placeholder={t("services.form.headerNamePlaceholder")}
            onChange={(event) => {
              form.updateHeader(row.id, "name", event.target.value);
              onChange();
            }}
          />
          <Input
            type="text"
            autoComplete="off"
            aria-label={t("services.form.headerValueNamed", {
              name: row.name || index + 1,
            })}
            value={row.value}
            maxLength={8_192}
            disabled={disabled}
            invalid={!row.existing && row.value.trim() === ""}
            placeholder={
              row.existing
                ? t("services.form.headerKeepValue")
                : t("services.form.headerValuePlaceholder")
            }
            onChange={(event) => {
              form.updateHeader(row.id, "value", event.target.value);
              onChange();
            }}
          />
          <Button
            variant="ghost"
            size="sm"
            disabled={disabled}
            aria-label={t("services.form.removeHeaderNamed", {
              name: row.name || index + 1,
            })}
            onClick={() => {
              form.removeHeader(row.id);
              onChange();
            }}
          >
            <Trash2 className="h-4 w-4" aria-hidden="true" />
          </Button>
        </div>
      ))}
      <Button
        variant="secondary"
        size="sm"
        className="self-start"
        disabled={disabled}
        onClick={() => {
          form.addHeader();
          onChange();
        }}
      >
        <Plus className="h-4 w-4" aria-hidden="true" />
        {t("services.form.addHeader")}
      </Button>
      {form.invalidHeaders ? (
        <p role="alert" className="text-caption text-danger">
          {t("services.form.headerRequired")}
        </p>
      ) : null}
    </section>
  );
}
