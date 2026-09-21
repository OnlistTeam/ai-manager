import type { FormEventHandler, KeyboardEventHandler, RefObject } from "react";
import { useTranslation } from "react-i18next";
import { Field } from "@/shared/ui/Field";
import { Input } from "@/shared/ui/Input";
import { ProviderConnectFailure } from "./ProviderConnectFailure";

interface ProviderPresetConnectFormProps {
  formId: string;
  nameId: string;
  keyId: string;
  modelId: string;
  name: string;
  apiKey: string;
  model: string;
  modelRequired: boolean;
  invalid: { name: boolean; key: boolean; model: boolean };
  busy: boolean;
  error: Error | null;
  keyRef: RefObject<HTMLInputElement>;
  onNameChange: (value: string) => void;
  onKeyChange: (value: string) => void;
  onModelChange: (value: string) => void;
  onSubmit: FormEventHandler<HTMLFormElement>;
  onKeyDown: KeyboardEventHandler<HTMLFormElement>;
}

export function ProviderPresetConnectForm({
  formId,
  nameId,
  keyId,
  modelId,
  name,
  apiKey,
  model,
  modelRequired,
  invalid,
  busy,
  error,
  keyRef,
  onNameChange,
  onKeyChange,
  onModelChange,
  onSubmit,
  onKeyDown,
}: ProviderPresetConnectFormProps) {
  const { t } = useTranslation();
  return (
    <form
      id={formId}
      className="flex flex-col gap-2.5"
      onSubmit={onSubmit}
      onKeyDown={onKeyDown}
    >
      {error ? <ProviderConnectFailure error={error} /> : null}

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
        hint={t("services.connect.keyHint")}
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
