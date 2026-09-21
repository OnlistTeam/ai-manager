import { ArrowLeft } from "lucide-react";
import { useRef, useState, type FormEvent, type KeyboardEvent } from "react";
import { useTranslation } from "react-i18next";
import type { ProviderCustomCreateDraft } from "@/entities/provider";
import { Button } from "@/shared/ui/Button";
import { Field } from "@/shared/ui/Field";
import { Input } from "@/shared/ui/Input";
import { Modal } from "@/shared/ui/Modal";
import { ProviderConnectFailure } from "./ProviderConnectFailure";
import { ServiceActionsPausedNotice } from "./ServiceActionsPausedNotice";

interface ProviderCustomConnectModalProps {
  toolName: string;
  modelRequired: boolean;
  busy: boolean;
  error: Error | null;
  mutationsBlocked: boolean;
  onOpenChange: (open: boolean) => void;
  onBack: () => void;
  onErrorReset?: () => void;
  onSubmit: (draft: ProviderCustomCreateDraft) => void;
}

const FORM_ID = "service-custom-connect-form";

function validHttpsBaseUrl(value: string): boolean {
  try {
    const url = new URL(value);
    return (
      url.protocol === "https:" &&
      url.hostname.length > 0 &&
      url.username.length === 0 &&
      url.password.length === 0 &&
      !value.includes("?") &&
      !value.includes("#") &&
      url.search.length === 0 &&
      url.hash.length === 0
    );
  } catch {
    return false;
  }
}

export function ProviderCustomConnectModal({
  toolName,
  modelRequired,
  busy,
  error,
  mutationsBlocked,
  onOpenChange,
  onBack,
  onErrorReset,
  onSubmit,
}: ProviderCustomConnectModalProps) {
  const { t } = useTranslation();
  const [name, setName] = useState("");
  const [baseUrl, setBaseUrl] = useState("");
  const [apiKey, setApiKey] = useState("");
  const [model, setModel] = useState("");
  const [attempted, setAttempted] = useState(false);
  const nameRef = useRef<HTMLInputElement>(null);
  const trimmed = {
    name: name.trim(),
    baseUrl: baseUrl.trim(),
    apiKey: apiKey.trim(),
    model: model.trim(),
  };
  const invalid = {
    name: attempted && trimmed.name === "",
    baseUrl: attempted && !validHttpsBaseUrl(trimmed.baseUrl),
    apiKey: attempted && trimmed.apiKey === "",
    model: attempted && modelRequired && trimmed.model === "",
  };
  const submitDraft = () => {
    if (mutationsBlocked) return;
    setAttempted(true);
    if (
      trimmed.name === "" ||
      !validHttpsBaseUrl(trimmed.baseUrl) ||
      trimmed.apiKey === "" ||
      (modelRequired && trimmed.model === "")
    ) {
      return;
    }
    onSubmit(trimmed);
  };
  const submit = (event: FormEvent<HTMLFormElement>) => {
    event.preventDefault();
    submitDraft();
  };
  const submitOnEnter = (event: KeyboardEvent<HTMLFormElement>) => {
    if (event.key !== "Enter" || event.nativeEvent.isComposing) return;
    event.preventDefault();
    submitDraft();
  };
  const update = (setter: (value: string) => void, value: string) => {
    setter(value);
    if (error) onErrorReset?.();
  };

  return (
    <Modal
      open
      size="lg"
      dismissible={!busy}
      initialFocusRef={nameRef}
      onOpenChange={onOpenChange}
      title={t("services.connect.customTitle")}
      description={t("services.connect.customDescription", { tool: toolName })}
      footer={
        <>
          <Button variant="secondary" disabled={busy} onClick={onBack}>
            <ArrowLeft className="h-4 w-4" aria-hidden="true" />
            {t("services.connect.backToPresets")}
          </Button>
          <Button
            type="submit"
            form={FORM_ID}
            disabled={mutationsBlocked}
            loading={busy}
          >
            {t(error ? "ds.action.retry" : "ds.action.connect")}
          </Button>
        </>
      }
    >
      {mutationsBlocked ? <ServiceActionsPausedNotice /> : null}
      <div className="mb-4 rounded-xl border border-warning/30 bg-warning/10 p-3 text-caption leading-5 text-content-muted">
        {t("services.connect.customCompatibility", { tool: toolName })}
      </div>
      <form
        id={FORM_ID}
        className="flex flex-col gap-2.5"
        onSubmit={submit}
        onKeyDown={submitOnEnter}
      >
        {error ? <ProviderConnectFailure error={error} /> : null}
        <Field
          id="service-custom-name"
          label={t("services.connect.name")}
          error={invalid.name ? t("services.connect.nameRequired") : undefined}
        >
          <Input
            ref={nameRef}
            id="service-custom-name"
            value={name}
            invalid={invalid.name}
            disabled={busy}
            onChange={(event) => update(setName, event.target.value)}
          />
        </Field>
        <Field
          id="service-custom-url"
          label={t("services.connect.customBaseUrl")}
          hint={t("services.connect.customBaseUrlHint")}
          error={
            invalid.baseUrl
              ? t("services.connect.customBaseUrlInvalid")
              : undefined
          }
        >
          <Input
            id="service-custom-url"
            type="url"
            value={baseUrl}
            invalid={invalid.baseUrl}
            disabled={busy}
            placeholder="https://api.example.com/v1"
            spellCheck={false}
            onChange={(event) => update(setBaseUrl, event.target.value)}
          />
        </Field>
        <Field
          id="service-custom-key"
          label={t("services.connect.key")}
          hint={t("services.connect.keyHint")}
          error={invalid.apiKey ? t("services.connect.keyRequired") : undefined}
        >
          <Input
            id="service-custom-key"
            type="text"
            autoComplete="off"
            value={apiKey}
            invalid={invalid.apiKey}
            disabled={busy}
            placeholder={t("services.connect.keyPlaceholder")}
            onChange={(event) => update(setApiKey, event.target.value)}
          />
        </Field>
        <Field
          id="service-custom-model"
          label={t("services.connect.model")}
          hint={modelRequired ? undefined : t("services.form.modelsHintSingle")}
          error={
            invalid.model ? t("services.connect.modelRequired") : undefined
          }
        >
          <Input
            id="service-custom-model"
            value={model}
            invalid={invalid.model}
            disabled={busy}
            spellCheck={false}
            onChange={(event) => update(setModel, event.target.value)}
          />
        </Field>
      </form>
    </Modal>
  );
}
