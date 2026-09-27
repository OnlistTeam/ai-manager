import { ExternalLink } from "lucide-react";
import type { FormEventHandler, KeyboardEventHandler, RefObject } from "react";
import { useTranslation } from "react-i18next";
import { Field } from "@/shared/ui/Field";
import { Input } from "@/shared/ui/Input";
import { cn } from "@/shared/ui/cn";
import { FOCUS_RING } from "@/shared/ui/focusRing";
import { ProviderConnectFailure } from "./ProviderConnectFailure";
import { trailingVersionSegment } from "./providerEndpointRouteUtils";

interface ProviderPresetConnectFormProps {
  formId: string;
  nameId: string;
  baseUrlId: string;
  keyId: string;
  modelId: string;
  name: string;
  baseUrl: string;
  apiKey: string;
  model: string;
  modelRequired: boolean;
  /** `ProviderConnectionProfile.baseUrlTakesNoVersion`. */
  baseUrlTakesNoVersion: boolean;
  /**
   * A preset's address is shown but not editable: it is what the backend
   * writes, and a different address belongs to the custom entry instead.
   */
  baseUrlReadOnly: boolean;
  /** A server on this machine usually takes no key. */
  keyOptional: boolean;
  /** Opens the selected service's key page; absent for a custom address. */
  onOpenKeyPage?: () => void;
  invalid: { name: boolean; baseUrl: boolean; key: boolean; model: boolean };
  busy: boolean;
  error: Error | null;
  keyRef: RefObject<HTMLInputElement>;
  baseUrlRef?: RefObject<HTMLInputElement>;
  onNameChange: (value: string) => void;
  onBaseUrlChange: (value: string) => void;
  onKeyChange: (value: string) => void;
  onModelChange: (value: string) => void;
  onSubmit: FormEventHandler<HTMLFormElement>;
  onKeyDown: KeyboardEventHandler<HTMLFormElement>;
}

export function ProviderPresetConnectForm({
  formId,
  nameId,
  baseUrlId,
  keyId,
  modelId,
  name,
  baseUrl,
  apiKey,
  model,
  modelRequired,
  baseUrlTakesNoVersion,
  baseUrlReadOnly,
  keyOptional,
  onOpenKeyPage,
  invalid,
  busy,
  error,
  keyRef,
  baseUrlRef,
  onNameChange,
  onBaseUrlChange,
  onKeyChange,
  onModelChange,
  onSubmit,
  onKeyDown,
}: ProviderPresetConnectFormProps) {
  const { t } = useTranslation();
  const versionSegment =
    baseUrlTakesNoVersion && !baseUrlReadOnly
      ? trailingVersionSegment(baseUrl)
      : null;
  const baseUrlWarning =
    versionSegment === null
      ? undefined
      : t("services.form.baseUrlVersionDoubled", { segment: versionSegment });
  return (
    <form
      id={formId}
      className="flex flex-col gap-2.5"
      onSubmit={onSubmit}
      onKeyDown={onKeyDown}
    >
      {error ? <ProviderConnectFailure error={error} /> : null}

      <Field
        id={baseUrlId}
        label={t("services.connect.baseUrl")}
        hint={
          baseUrlWarning || baseUrlReadOnly
            ? undefined
            : t("services.connect.customBaseUrlHint")
        }
        warning={baseUrlWarning}
        error={
          invalid.baseUrl ? t("services.connect.baseUrlInvalid") : undefined
        }
      >
        <Input
          ref={baseUrlRef}
          id={baseUrlId}
          type="url"
          value={baseUrl}
          readOnly={baseUrlReadOnly}
          invalid={invalid.baseUrl}
          disabled={busy}
          spellCheck={false}
          placeholder={
            baseUrlReadOnly ? undefined : "https://api.example.com/v1"
          }
          className={cn(
            "font-mono",
            baseUrlReadOnly && "bg-layer-2 text-content-muted",
          )}
          onChange={(event) => onBaseUrlChange(event.target.value)}
        />
      </Field>

      <Field
        id={nameId}
        label={t("services.connect.name")}
        error={invalid.name ? t("services.connect.nameRequired") : undefined}
      >
        <Input
          id={nameId}
          value={name}
          invalid={invalid.name}
          disabled={busy}
          onChange={(event) => onNameChange(event.target.value)}
        />
      </Field>

      <Field
        id={keyId}
        label={t("services.connect.key")}
        hint={t(
          keyOptional
            ? "services.connect.keyOptionalHint"
            : "services.connect.keyHint",
        )}
        error={invalid.key ? t("services.connect.keyRequired") : undefined}
      >
        <Input
          ref={keyRef}
          id={keyId}
          type="text"
          autoComplete="off"
          placeholder={t("services.connect.keyPlaceholder")}
          value={apiKey}
          invalid={invalid.key}
          disabled={busy}
          onChange={(event) => onKeyChange(event.target.value)}
        />
      </Field>

      {/* Outside the Field: inside it, this label joins the input's accessible
          description and a screen reader reads the label, the hint and "Get
          API key" as one sentence. */}
      {onOpenKeyPage ? (
        <button
          type="button"
          onClick={onOpenKeyPage}
          className={cn(
            "-mt-1 inline-flex w-fit items-center gap-1 rounded-sm text-caption font-medium text-brand hover:text-brand-hover",
            FOCUS_RING,
          )}
        >
          {t("services.connect.getKey")}
          <ExternalLink className="h-3.5 w-3.5" aria-hidden="true" />
        </button>
      ) : null}

      <Field
        id={modelId}
        label={t("services.connect.model")}
        hint={modelRequired ? undefined : t("services.form.modelsHintSingle")}
        error={invalid.model ? t("services.connect.modelRequired") : undefined}
      >
        <Input
          id={modelId}
          value={model}
          invalid={invalid.model}
          disabled={busy}
          spellCheck={false}
          onChange={(event) => onModelChange(event.target.value)}
        />
      </Field>
    </form>
  );
}
