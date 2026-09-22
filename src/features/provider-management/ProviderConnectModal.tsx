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
  ProviderConnectionProfile,
  ProviderCreateDraft,
  ProviderCustomCreateDraft,
  ToolId,
} from "@/entities/provider";
import { native } from "@/native";
import { toErrorCopy } from "@/shared/lib/nativeError";
import { Modal } from "@/shared/ui/Modal";
import { ProviderConnectModalFooter } from "./ProviderConnectModalFooter";
import { normalizeProviderEndpoint } from "./providerEndpointRouteUtils";
import { ProviderPresetPicker } from "./ProviderPresetPicker";
import { ProviderPresetConnectForm } from "./ProviderPresetConnectForm";
import { ServiceActionsPausedNotice } from "./ServiceActionsPausedNotice";
import { useProviderPresetSpeedTest } from "./useProviderPresetSpeedTest";

export interface ProviderConnectModalProps {
  profile: ProviderConnectionProfile | null;
  tool: ToolId | null;
  toolName: string;
  busy?: boolean;
  error?: Error | null;
  mutationsBlocked?: boolean;
  preferCompatible?: boolean;
  /** Legacy caller compatibility; endpoint creation is always available. */
  advancedMode?: boolean;
  onOpenChange: (open: boolean) => void;
  onErrorReset?: () => void;
  onSubmit: (draft: ProviderCreateDraft) => void;
  onCustomSubmit?: (draft: ProviderCustomCreateDraft) => void;
}

const FORM_ID = "service-connect-form";
const PRESET_ID = "service-connect-preset";
const NAME_ID = "service-connect-name";
const BASE_URL_ID = "service-connect-base-url";
const KEY_ID = "service-connect-key";
const MODEL_ID = "service-connect-model";

export function ProviderConnectModal({
  profile,
  tool,
  toolName,
  busy = false,
  error = null,
  mutationsBlocked = false,
  preferCompatible = false,
  onOpenChange,
  onErrorReset,
  onSubmit,
  onCustomSubmit,
}: ProviderConnectModalProps) {
  const { t } = useTranslation();
  const [presetId, setPresetId] = useState("");
  const [name, setName] = useState("");
  const [baseUrl, setBaseUrl] = useState("");
  const [key, setKey] = useState("");
  const [model, setModel] = useState("");
  const [attempted, setAttempted] = useState(false);
  const keyRef = useRef<HTMLInputElement>(null);
  const speedTest = useProviderPresetSpeedTest(tool);

  useEffect(() => {
    const defaultPreset = profile?.presets.find(
      (candidate) => candidate.id === profile.defaultPresetId,
    );
    const preset = preferCompatible
      ? (profile?.presets.find((candidate) => !candidate.official) ??
        defaultPreset)
      : defaultPreset;
    setPresetId(preset?.id ?? "");
    setName(preset?.defaultName ?? "");
    setBaseUrl(preset?.baseUrl ?? "");
    setKey("");
    setModel(preset?.defaultModel ?? "");
    setAttempted(false);
  }, [preferCompatible, profile]);

  const selectedPreset =
    profile?.presets.find((preset) => preset.id === presetId) ??
    profile?.presets.find((preset) => preset.id === profile.defaultPresetId) ??
    profile?.presets[0] ??
    null;
  if (profile === null || selectedPreset === null) return null;

  const modelRequired = profile.modelRequired;
  /*
   * One form, two create paths. The address is always visible and always
   * editable; touching it is what turns a preset into a custom endpoint, so
   * there is no mode to switch and no second dialog to find.
   *
   * The security rule that used to justify hiding the address is unchanged,
   * because it lives on the submit: an untouched address sends only the preset
   * id and the backend supplies the endpoint, while an edited one goes through
   * the custom path and is recorded as custom. The renderer still cannot pass
   * its own URL off as a preset.
   */
  const normalizedAddress = normalizeProviderEndpoint(baseUrl);
  const addressEdited =
    normalizedAddress !== normalizeProviderEndpoint(selectedPreset.baseUrl);
  const addressUsable = !addressEdited || normalizedAddress !== null;
  const invalid = {
    name: attempted && name.trim() === "",
    baseUrl: attempted && !addressUsable,
    key: attempted && key.trim() === "",
    model: attempted && modelRequired && model.trim() === "",
  };
  const submitDraft = () => {
    if (mutationsBlocked) return;
    setAttempted(true);
    if (
      name.trim() === "" ||
      !addressUsable ||
      key.trim() === "" ||
      (modelRequired && model.trim() === "")
    ) {
      return;
    }
    if (addressEdited && onCustomSubmit && normalizedAddress !== null) {
      onCustomSubmit({
        name: name.trim(),
        baseUrl: normalizedAddress,
        apiKey: key.trim(),
        model: model.trim(),
      });
      return;
    }
    onSubmit({
      presetId: selectedPreset.id,
      name: name.trim(),
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

  return (
    <Modal
      open
      onOpenChange={onOpenChange}
      dismissible={!busy}
      initialFocusRef={keyRef}
      size="lg"
      title={t("services.connect.title", {
        service: selectedPreset.serviceName,
      })}
      description={t("services.connect.description", { tool: toolName })}
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

      <ProviderPresetPicker
        id={PRESET_ID}
        presets={profile.presets}
        value={selectedPreset.id}
        disabled={busy}
        measurements={speedTest.results}
        measured={speedTest.measured}
        measuring={speedTest.testing}
        measurementFailed={speedTest.error !== null}
        onMeasure={speedTest.run}
        onChange={(nextId) => {
          const next = profile.presets.find((preset) => preset.id === nextId);
          if (!next) return;
          setPresetId(next.id);
          setName(next.defaultName);
          setBaseUrl(next.baseUrl);
          setModel(next.defaultModel);
          setKey("");
          setAttempted(false);
          onErrorReset?.();
          requestAnimationFrame(() => keyRef.current?.focus());
        }}
      />

      <div className="mt-3 flex flex-col gap-3">
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
          addressEdited={addressEdited}
          keyUrlLabel={t(
            selectedPreset.official
              ? "services.connect.getKey"
              : "services.connect.openProvider",
          )}
          onOpenKeyPage={() => {
            if (tool === null) return;
            void native.providers
              .openPresetKeyPage(tool, selectedPreset.id)
              .catch((failure: unknown) => {
                const copy = toErrorCopy(failure);
                toast.error(t(copy.messageKey), {
                  description: copy.remediationKey
                    ? t(copy.remediationKey)
                    : undefined,
                });
              });
          }}
          modelRequired={modelRequired}
          invalid={invalid}
          busy={busy}
          error={error}
          keyRef={keyRef}
          onBaseUrlChange={(value) => {
            setBaseUrl(value);
            onErrorReset?.();
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
