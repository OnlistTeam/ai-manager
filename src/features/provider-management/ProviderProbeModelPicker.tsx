import { ChevronDown, RefreshCw } from "lucide-react";
import { useTranslation } from "react-i18next";

import type { ModelCatalog } from "@/native";
import { toErrorCopy } from "@/shared/lib/nativeError";
import { Button } from "@/shared/ui/Button";
import { Field } from "@/shared/ui/Field";
import { Input } from "@/shared/ui/Input";

const SELECT_CLASS =
  "h-10 w-full appearance-none rounded-lg border border-hairline bg-layer-1 pl-3 pr-9 font-mono text-body text-content focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-brand disabled:cursor-not-allowed disabled:opacity-60";

interface ProviderProbeModelPickerProps {
  catalog: ModelCatalog | undefined;
  loading: boolean;
  /** Set when the catalogue request itself failed. A refusal is not an error. */
  error: Error | null;
  model: string;
  disabled: boolean;
  onModelChange: (model: string) => void;
  onReload: () => void;
}

/**
 * A dropdown when the service publishes a catalogue, a text box when it does
 * not. Plenty of relays serve no `/v1/models`, and refusing to test them would
 * be a worse answer than letting the user type the name.
 */
export function ProviderProbeModelPicker({
  catalog,
  loading,
  error,
  model,
  disabled,
  onModelChange,
  onReload,
}: ProviderProbeModelPickerProps) {
  const { t } = useTranslation();
  const models = catalog?.models ?? [];
  const manual = !loading && models.length === 0;

  if (loading) {
    return (
      <p role="status" className="text-caption text-content-muted">
        {t("services.probe.modelLoading")}
      </p>
    );
  }

  return (
    <div className="flex flex-col gap-1.5">
      <div className="flex items-end gap-2">
        {manual ? (
          <Field
            id="probe-model"
            label={t("services.probe.modelManual")}
            className="min-w-0 flex-1"
          >
            <Input
              id="probe-model"
              value={model}
              maxLength={256}
              spellCheck={false}
              disabled={disabled}
              className="font-mono"
              onChange={(event) => onModelChange(event.target.value)}
            />
          </Field>
        ) : (
          <Field
            id="probe-model"
            label={t("services.probe.model")}
            className="min-w-0 flex-1"
          >
            <div className="relative">
              <select
                id="probe-model"
                value={model}
                disabled={disabled}
                className={SELECT_CLASS}
                onChange={(event) => onModelChange(event.target.value)}
              >
                {models.map((entry) => (
                  <option key={entry.id} value={entry.id}>
                    {entry.id}
                  </option>
                ))}
              </select>
              <ChevronDown
                className="pointer-events-none absolute right-3 top-3 h-4 w-4 text-content-muted"
                aria-hidden="true"
              />
            </div>
          </Field>
        )}
        <Button
          variant="ghost"
          disabled={disabled}
          aria-label={t("services.probe.modelReload")}
          onClick={onReload}
        >
          <RefreshCw className="h-4 w-4" aria-hidden="true" />
          {t("services.probe.modelReload")}
        </Button>
      </div>

      {manual ? (
        <p className="text-caption text-content-muted">
          {t("services.probe.modelManualHint")}
        </p>
      ) : (
        <p className="text-caption text-content-muted">
          {t("services.probe.modelCount", { count: models.length })}
          {catalog?.truncated ? (
            <> {t("services.probe.modelTruncated", { count: models.length })}</>
          ) : null}
        </p>
      )}

      {catalog?.rejection ? (
        <p role="status" className="text-caption text-warning">
          {t("services.probe.modelRefused", {
            status: catalog.rejection.status,
          })}
        </p>
      ) : null}

      {error ? (
        <p role="status" className="text-caption text-content-muted">
          {t(toErrorCopy(error).messageKey)}
        </p>
      ) : null}
    </div>
  );
}
