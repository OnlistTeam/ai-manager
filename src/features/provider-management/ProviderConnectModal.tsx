import {
  useEffect,
  useRef,
  useState,
  type FormEvent,
  type KeyboardEvent,
} from "react";
import { useTranslation } from "react-i18next";
import { toast } from "sonner";
import type {
  ProviderConnectionPreset,
  ProviderConnectionProfile,
  ProviderCreateDraft,
  ProviderCustomCreateDraft,
  ToolId,
} from "@/entities/provider";
import { native } from "@/native";
import { toErrorCopy } from "@/shared/lib/nativeError";
import { Modal } from "@/shared/ui/Modal";
import { ProviderConnectModalFooter } from "./ProviderConnectModalFooter";
import {
  isLoopbackEndpoint,
  normalizeProviderEndpoint,
} from "./providerEndpointRouteUtils";
import { ProviderPresetConnectForm } from "./ProviderPresetConnectForm";
import { ServiceActionsPausedNotice } from "./ServiceActionsPausedNotice";

/** What the add page handed over: one catalogue entry, or an address of the user's own. */
export type ProviderConnectTarget =
  | { kind: "preset"; preset: ProviderConnectionPreset }
  | { kind: "custom" };

export interface ProviderConnectModalProps {
  /** `null` keeps the dialog closed. */
  target: ProviderConnectTarget | null;
  profile: ProviderConnectionProfile | null;
  tool: ToolId | null;
  toolName: string;
  busy?: boolean;
  error?: Error | null;
  mutationsBlocked?: boolean;
  onOpenChange: (open: boolean) => void;
  onErrorReset?: () => void;
  onSubmit: (draft: ProviderCreateDraft) => void;
  onCustomSubmit: (draft: ProviderCustomCreateDraft) => void;
}

const FORM_ID = "service-connect-form";
const NAME_ID = "service-connect-name";
const BASE_URL_ID = "service-connect-base-url";
const KEY_ID = "service-connect-key";
const MODEL_ID = "service-connect-model";

/**
 * The second layer of the add page (ADR-0057): the fields one card needs.
 *
 * A preset's address is shown read-only. It is exactly what the backend will
 * write, so the user always sees where the key goes, and the submit still
 * sends only the preset id: the renderer cannot pass an address of its own
 * off as a preset. A different address is what the Custom card is for, and it
 * goes through the custom create path, which records it as custom.
 */
export function ProviderConnectModal({
  target,
  profile,
  tool,
  toolName,
  busy = false,
  error = null,
  mutationsBlocked = false,
  onOpenChange,
  onErrorReset,
  onSubmit,
  onCustomSubmit,
}: ProviderConnectModalProps) {
  const { t } = useTranslation();
  const [name, setName] = useState("");
  const [baseUrl, setBaseUrl] = useState("");
  const [key, setKey] = useState("");
  const [model, setModel] = useState("");
  const [attempted, setAttempted] = useState(false);
  const keyRef = useRef<HTMLInputElement>(null);
  const baseUrlRef = useRef<HTMLInputElement>(null);
  const preset = target?.kind === "preset" ? target.preset : null;

  useEffect(() => {
    setName(preset?.defaultName ?? "");
    setBaseUrl(preset?.baseUrl ?? "");
    setKey("");
    setModel(preset?.defaultModel ?? "");
    setAttempted(false);
  }, [preset, target]);

  if (target === null || profile === null) return null;

  const modelRequired = profile.modelRequired;
  const normalizedAddress = preset ? null : normalizeProviderEndpoint(baseUrl);
  // A server on this machine has no account to hand out keys, so it is the
  // one case a key may be left blank; native writes a placeholder instead.
  const keyOptional = preset
    ? preset.kind === "local"
    : normalizedAddress !== null && isLoopbackEndpoint(normalizedAddress);
  const invalid = {
    name: attempted && name.trim() === "",
    baseUrl: attempted && preset === null && normalizedAddress === null,
    key: attempted && !keyOptional && key.trim() === "",
    model: attempted && modelRequired && model.trim() === "",
  };
  const submitDraft = () => {
    if (mutationsBlocked) return;
    setAttempted(true);
    if (
      name.trim() === "" ||
      (!keyOptional && key.trim() === "") ||
      (modelRequired && model.trim() === "")
    ) {
      return;
    }
    if (preset) {
      onSubmit({
        presetId: preset.id,
        name: name.trim(),
        apiKey: key.trim(),
        model: model.trim(),
      });
      return;
    }
    if (normalizedAddress === null) return;
    onCustomSubmit({
      name: name.trim(),
      baseUrl: normalizedAddress,
      apiKey: key.trim(),
      model: model.trim(),
    });
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
  const openKeyPage = () => {
    if (tool === null || preset === null) return;
    void native.providers
      .openPresetKeyPage(tool, preset.id)
      .catch((failure: unknown) => {
        const copy = toErrorCopy(failure);
        toast.error(t(copy.messageKey), {
          description: copy.remediationKey ? t(copy.remediationKey) : undefined,
        });
      });
  };

  return (
    <Modal
      open
      onOpenChange={onOpenChange}
      dismissible={!busy}
      initialFocusRef={preset ? keyRef : baseUrlRef}
      title={
        preset
          ? t("services.connect.title", { service: preset.serviceName })
          : t("services.connect.customTitle")
      }
      description={t(
        preset
          ? "services.connect.description"
          : "services.connect.customDescription",
        { tool: toolName },
      )}
      footer={
        <ProviderConnectModalFooter
          formId={FORM_ID}
          busy={busy}
          mutationsBlocked={mutationsBlocked}
          error={error}
          onCancel={() => onOpenChange(false)}
        />
      }
    >
      {mutationsBlocked ? <ServiceActionsPausedNotice /> : null}

      <div className="flex flex-col gap-3">
        <ProviderPresetConnectForm
          formId={FORM_ID}
          nameId={NAME_ID}
          baseUrlId={BASE_URL_ID}
          keyId={KEY_ID}
          modelId={MODEL_ID}
          name={name}
          baseUrl={baseUrl}
          apiKey={key}
          model={model}
          baseUrlReadOnly={preset !== null}
          keyOptional={keyOptional}
          onOpenKeyPage={
            preset && preset.kind !== "local" ? openKeyPage : undefined
          }
          modelRequired={modelRequired}
          baseUrlTakesNoVersion={profile.baseUrlTakesNoVersion}
          invalid={invalid}
          busy={busy}
          error={error}
          keyRef={keyRef}
          baseUrlRef={baseUrlRef}
          onBaseUrlChange={(value) => {
            setBaseUrl(value);
            if (error) onErrorReset?.();
          }}
          onNameChange={(value) => {
            setName(value);
            if (error) onErrorReset?.();
          }}
          onKeyChange={(value) => {
            setKey(value);
            if (error) onErrorReset?.();
          }}
          onModelChange={(value) => {
            setModel(value);
            if (error) onErrorReset?.();
          }}
          onSubmit={submit}
          onKeyDown={submitOnEnter}
        />

        {/* The one non-obvious thing about saving: it does not verify the key. */}
        <p className="text-caption text-content-muted">
          {t("services.connect.afterSave", { tool: toolName })}
        </p>
      </div>
    </Modal>
  );
}
