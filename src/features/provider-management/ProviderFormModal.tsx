import {
  useEffect,
  useRef,
  useState,
  type FormEvent,
  type KeyboardEvent,
} from "react";
import { AlertCircle } from "lucide-react";
import { useTranslation } from "react-i18next";
import type {
  Provider,
  ProviderDraft,
  ProviderEditProfile,
} from "@/entities/provider";
import { toErrorCopy } from "@/shared/lib/nativeError";
import { Button } from "@/shared/ui/Button";
import { CopyableInput } from "@/shared/ui/CopyableInput";
import { Field } from "@/shared/ui/Field";
import { Input } from "@/shared/ui/Input";
import { Modal } from "@/shared/ui/Modal";
import { ServiceActionsPausedNotice } from "./ServiceActionsPausedNotice";
import { ProviderSettingsFields } from "./ProviderSettingsFields";
import { useProviderSettingsForm } from "./useProviderSettingsForm";

export interface ProviderFormModalProps {
  /** `null` = closed. Openness and payload are merged into a single prop, matching the existing modal convention. */
  provider: Provider | null;
  profile?: ProviderEditProfile | null;
  profileLoading?: boolean;
  profileError?: Error | null;
  /** Legacy caller compatibility; supported endpoint fields are always shown. */
  advancedMode?: boolean;
  busy?: boolean;
  error?: Error | null;
  mutationsBlocked?: boolean;
  onOpenChange: (open: boolean) => void;
  onErrorReset?: () => void;
  onSubmit: (draft: ProviderDraft) => void;
}

const NAME_ID = "service-name";
const KEY_ID = "service-key";
const FORM_ID = "service-edit-form";

export function ProviderFormModal({
  provider,
  profile = null,
  profileLoading = false,
  profileError = null,
  busy = false,
  error = null,
  mutationsBlocked = false,
  onOpenChange,
  onErrorReset,
  onSubmit,
}: ProviderFormModalProps) {
  const { t } = useTranslation();
  const [name, setName] = useState("");
  const [key, setKey] = useState("");
  const [touchedEmptyName, setTouchedEmptyName] = useState(false);
  const nameRef = useRef<HTMLInputElement>(null);
  const currentProfile = profile?.providerId === provider?.id ? profile : null;
  const settings = useProviderSettingsForm(
    currentProfile,
    provider?.id ?? null,
    provider?.tool ?? null,
  );

  // Reset the form whenever the provider changes: keeping the previous entry's input around is a source of data mishaps.
  useEffect(() => {
    setName(provider?.name ?? "");
    setKey(provider?.apiKey ?? "");
    setTouchedEmptyName(false);
  }, [provider?.id, provider?.name, provider?.apiKey]);

  if (provider === null) {
    return null;
  }

  const errorCopy = error ? toErrorCopy(error) : null;
  const errorTitle = t("services.form.errorTitle");

  const submitDraft = () => {
    if (mutationsBlocked) return;
    if (name.trim() === "") {
      setTouchedEmptyName(true);
      return;
    }
    const settingsPatch = settings.buildPatch();
    if (settingsPatch === null) return;
    // The key field is prefilled with the already-stored original. Leaving
    // it untouched, or clearing it, both send null — the backend keeps it
    // as-is based on that, so no unnecessary key rewrite ever happens. This
    // provider's key can only be replaced here, never cleared to empty.
    const keyUnchanged = key === (provider.apiKey ?? "") || key.trim() === "";
    onSubmit({
      name: name.trim(),
      apiKey: keyUnchanged ? null : key,
      ...settingsPatch,
    });
  };
  const submit = (event: FormEvent<HTMLFormElement>) => {
    event.preventDefault();
    submitDraft();
  };
  const submitOnEnter = (event: KeyboardEvent<HTMLFormElement>) => {
    if (event.key !== "Enter" || event.nativeEvent.isComposing) return;
    // Buttons (including Copy) keep their own keyboard behavior.
    if (!(event.target instanceof HTMLInputElement)) return;
    event.preventDefault();
    submitDraft();
  };

  return (
    <Modal
      open
      onOpenChange={onOpenChange}
      dismissible={!busy}
      initialFocusRef={nameRef}
      title={t("services.form.title")}
      description={t("services.form.description")}
      footer={
        <>
          <Button
            variant="ghost"
            disabled={busy}
            onClick={() => onOpenChange(false)}
          >
            {t("ds.action.cancel")}
          </Button>
          <Button
            type="submit"
            form={FORM_ID}
            disabled={mutationsBlocked}
            loading={busy}
            aria-label={errorCopy ? t("services.form.retrySave") : undefined}
          >
            {t(errorCopy ? "ds.action.retry" : "services.form.save")}
          </Button>
        </>
      }
    >
      {mutationsBlocked ? <ServiceActionsPausedNotice /> : null}

      <form
        id={FORM_ID}
        className="flex flex-col gap-4"
        onSubmit={submit}
        onKeyDown={submitOnEnter}
      >
        {errorCopy ? (
          <div
            role="alert"
            aria-label={errorTitle}
            className="rounded-lg border border-danger/25 bg-danger/10 p-3 text-caption text-content"
          >
            <p className="flex items-start gap-2 font-medium">
              <AlertCircle
                className="mt-0.5 h-4 w-4 shrink-0 text-danger"
                aria-hidden="true"
              />
              <span>{errorTitle}</span>
            </p>
            <p className="mt-1.5">{t(errorCopy.messageKey)}</p>
            {errorCopy.remediationKey ? (
              <p className="mt-1.5 text-content-muted">
                {t(errorCopy.remediationKey)}
              </p>
            ) : null}
            <p className="mt-1.5 text-content-muted">
              {t("services.form.retryHint")}
            </p>
          </div>
        ) : null}

        <Field
          id={NAME_ID}
          label={t("services.form.name")}
          hint={t("services.form.nameHint")}
          error={
            touchedEmptyName && name.trim() === ""
              ? t("services.form.nameRequired")
              : undefined
          }
        >
          <Input
            ref={nameRef}
            id={NAME_ID}
            value={name}
            invalid={touchedEmptyName && name.trim() === ""}
            disabled={busy}
            onChange={(event) => {
              setName(event.target.value);
              setTouchedEmptyName(false);
              if (error) onErrorReset?.();
            }}
          />
        </Field>

        <Field
          id={KEY_ID}
          label={t("services.form.key")}
          hint={t(
            provider.apiKey === null
              ? "services.form.keyHintEmpty"
              : "services.form.keyHint",
          )}
        >
          <CopyableInput
            id={KEY_ID}
            copyLabel={t("services.form.key")}
            type="text"
            autoComplete="off"
            spellCheck={false}
            className="font-mono text-mono-sm"
            value={key}
            disabled={busy}
            onChange={(event) => {
              setKey(event.target.value);
              if (error) onErrorReset?.();
            }}
          />
        </Field>

        <ProviderSettingsFields
          profile={currentProfile}
          loading={profileLoading}
          failed={profileError !== null}
          disabled={busy}
          form={settings}
          onChange={() => {
            if (error) onErrorReset?.();
          }}
        />
      </form>
    </Modal>
  );
}
