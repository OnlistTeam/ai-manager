import {
  useEffect,
  useRef,
  useState,
  type FormEvent,
  type KeyboardEvent,
} from "react";
import { useTranslation } from "react-i18next";
import type {
  ProviderConnectionProfile,
  ProviderCreateDraft,
  ProviderCustomCreateDraft,
  ToolId,
} from "@/entities/provider";
import { Modal } from "@/shared/ui/Modal";
import { ProviderConnectModalFooter } from "./ProviderConnectModalFooter";
import { ProviderConnectOverview } from "./ProviderConnectOverview";
import { ProviderCustomConnectModal } from "./ProviderCustomConnectModal";
import { ProviderCustomEntryBanner } from "./ProviderCustomEntryBanner";
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
  const [key, setKey] = useState("");
  const [model, setModel] = useState("");
  const [attempted, setAttempted] = useState(false);
  const [customMode, setCustomMode] = useState(
    Boolean(onCustomSubmit) && !preferCompatible,
  );
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
    setKey("");
    setModel(preset?.defaultModel ?? "");
    setAttempted(false);
  }, [preferCompatible, profile]);

  useEffect(() => {
    // Connection recovery intentionally starts in the compatible preset
    // catalog. Ordinary endpoint creation starts in the direct Base URL form.
    if (preferCompatible) setCustomMode(false);
  }, [preferCompatible]);

  const selectedPreset =
    profile?.presets.find((preset) => preset.id === presetId) ??
    profile?.presets.find((preset) => preset.id === profile.defaultPresetId) ??
    profile?.presets[0] ??
    null;
  if (profile === null || selectedPreset === null) return null;

  if (customMode && onCustomSubmit) {
    return (
      <ProviderCustomConnectModal
        toolName={toolName}
        modelRequired={profile.modelRequired}
        busy={busy}
        error={error}
        mutationsBlocked={mutationsBlocked}
        onOpenChange={onOpenChange}
        onBack={() => {
          setCustomMode(false);
          onErrorReset?.();
        }}
        onErrorReset={onErrorReset}
        onSubmit={onCustomSubmit}
      />
    );
  }

  const modelRequired = profile.modelRequired;
  const invalid = {
    name: attempted && name.trim() === "",
    key: attempted && key.trim() === "",
    model: attempted && modelRequired && model.trim() === "",
  };
  const submitDraft = () => {
    if (mutationsBlocked) return;
    setAttempted(true);
    if (
      name.trim() === "" ||
      key.trim() === "" ||
      (modelRequired && model.trim() === "")
    ) {
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

      {onCustomSubmit ? (
        <ProviderCustomEntryBanner
          busy={busy}
          onCustomEntry={() => {
            setCustomMode(true);
            onErrorReset?.();
          }}
        />
      ) : null}

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
          setModel(next.defaultModel);
          setKey("");
          setAttempted(false);
          onErrorReset?.();
          requestAnimationFrame(() => keyRef.current?.focus());
        }}
      />

      <div className="mt-3 flex flex-col gap-3">
        <ProviderConnectOverview preset={selectedPreset} toolName={toolName} />

        <ProviderPresetConnectForm
          formId={FORM_ID}
          nameId={NAME_ID}
          keyId={KEY_ID}
          modelId={MODEL_ID}
          name={name}
          apiKey={key}
          model={model}
          modelRequired={modelRequired}
          invalid={invalid}
          busy={busy}
          error={error}
          keyRef={keyRef}
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
      </div>
    </Modal>
  );
}
