import { useEffect, useMemo, useRef, useState } from "react";
import { Link2, Radio, Send } from "lucide-react";
import { useTranslation } from "react-i18next";

import { useProviderEditProfile } from "@/entities/provider";
import { MAX_PROBE_PROMPT_CHARS, type ProbeModelKind } from "@/native";
import { Button } from "@/shared/ui/Button";
import { Field } from "@/shared/ui/Field";
import { Input } from "@/shared/ui/Input";
import { Modal } from "@/shared/ui/Modal";
import { ScopeTabs } from "@/shared/ui/ScopeTabs";
import { ProviderActionError } from "./ProviderActionError";
import { ProviderProbeModelPicker } from "./ProviderProbeModelPicker";
import { ProviderProbeResult } from "./ProviderProbeResult";
import { ProviderProbeReachability } from "./ProviderProbeReachability";
import {
  subjectId,
  subjectName,
  useAdoptBaseUrl,
  useAdoptModel,
  useModelCatalog,
  useProbeModel,
  type ProbeSubject,
} from "./useProviderModelProbe";

const DEFAULT_PROMPT = "hi";

interface ProviderTestModalProps {
  /** `null` closes the dialog. One prop carries both openness and subject. */
  subject: ProbeSubject | null;
  mutationsBlocked?: boolean;
  onOpenChange: (open: boolean) => void;
}

/**
 * The one place a connection is tested (ADR-0041).
 *
 * The address check is the first line rather than a separate button: it is what
 * separates "this host is gone" from "this host answered and refused your key".
 * Everything below it costs the user money, so nothing below it runs on its own.
 *
 * It takes a subject rather than a service because a connection a shell profile
 * exports is the connection the tool will really use, and refusing to test it
 * would make the most confusing kind of setup the only untestable one. Such a
 * connection has no saved record, so it has no address check of its own and
 * nowhere to write a tested model back to; everything else is identical.
 */
export function ProviderTestModal({
  subject,
  mutationsBlocked = false,
  onOpenChange,
}: ProviderTestModalProps) {
  const { t } = useTranslation();
  const provider = subject?.kind === "provider" ? subject.provider : null;

  const catalog = useModelCatalog(subject);
  const profile = useProviderEditProfile(
    provider?.tool ?? null,
    provider?.id ?? null,
  );
  const probe = useProbeModel();
  const adopt = useAdoptModel();
  const adoptBaseUrl = useAdoptBaseUrl();

  const [model, setModel] = useState("");
  const [kindOverride, setKindOverride] = useState<ProbeModelKind | null>(null);
  const [prompt, setPrompt] = useState(DEFAULT_PROMPT);
  const promptRef = useRef<HTMLInputElement>(null);
  const identity = subject === null ? null : subjectId(subject);

  // A new subject resets the form; the previous connection's model and result
  // must never bleed into the next dialog.
  useEffect(() => {
    setModel("");
    setKindOverride(null);
    setPrompt(DEFAULT_PROMPT);
    probe.reset();
    adopt.reset();
    adoptBaseUrl.reset();
    // `probe` and `adopt` are stable mutation handles from React Query.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [identity]);

  const models = useMemo(() => catalog.data?.models ?? [], [catalog.data]);
  const suggested = useMemo(() => {
    const configured = profile.data?.models.find((saved) =>
      models.some((entry) => entry.id === saved),
    );
    return configured ?? models[0]?.id ?? profile.data?.models[0] ?? "";
  }, [models, profile.data]);

  // Adopt the suggestion only while the user has not chosen for themselves.
  useEffect(() => {
    if (model === "" && suggested !== "") setModel(suggested);
  }, [model, suggested]);

  /**
   * Image generation is offered when this endpoint itself shows evidence of it:
   * an OpenAI-dialect service, or any catalogue — whatever the dialect — that
   * lists a model this product recognises as an image model. A relay set up for
   * Claude Code that serves `openai/gpt-image-…` is exactly that second case,
   * and the old rule (OpenAI dialect only) hid the switch from it while the
   * name heuristic still selected Image, leaving no way back to text.
   */
  const imageAvailable =
    catalog.data?.protocol === "openAi" ||
    catalog.data?.protocol === "openAiResponses" ||
    models.some((entry) => entry.kind === "image");
  const catalogKind =
    models.find((entry) => entry.id === model)?.kind ?? "text";
  // Never leave the request on a kind the user has no control to change.
  const kind = imageAvailable ? (kindOverride ?? catalogKind) : "text";

  const busy = probe.isPending || adopt.isPending || adoptBaseUrl.isPending;
  const canSend =
    subject !== null && model.trim() !== "" && prompt.trim() !== "" && !busy;
  const canAdopt =
    provider !== null &&
    profile.data?.capabilities.canEditModels === true &&
    model.trim() !== "" &&
    !profile.data.models.includes(model) &&
    !mutationsBlocked;

  /**
   * An address the backend reached successfully after the saved one failed.
   * Never derived here: the renderer shows what was verified and nothing else,
   * which is what keeps this from becoming a guess that fires on every error.
   */
  const suggestedBaseUrl = probe.data?.suggestedBaseUrl ?? null;
  const canAdoptBaseUrl =
    profile.data?.capabilities.canEditBaseUrl === true && !mutationsBlocked;

  return (
    <Modal
      open={subject !== null}
      onOpenChange={onOpenChange}
      dismissible={!busy}
      size="lg"
      title={t("services.probe.title", {
        name: subject === null ? "" : subjectName(subject),
      })}
      initialFocusRef={promptRef}
    >
      {subject === null ? null : (
        <div className="flex flex-col gap-4">
          {provider === null ? (
            <p className="text-caption text-content-muted">
              {t("services.probe.effectiveNote")}
            </p>
          ) : (
            <ProviderProbeReachability provider={provider} />
          )}

          <ProviderProbeModelPicker
            catalog={catalog.data}
            loading={catalog.isPending}
            refreshing={catalog.isFetching && !catalog.isPending}
            error={catalog.error}
            model={model}
            disabled={busy}
            onModelChange={(next) => {
              setModel(next);
              setKindOverride(null);
              probe.reset();
            }}
            onReload={() => void catalog.refetch()}
          />

          {imageAvailable ? (
            <div className="flex flex-col gap-1.5">
              <p className="text-caption text-content">
                {t("services.probe.kind")}
              </p>
              <ScopeTabs
                label={t("services.probe.kind")}
                items={[
                  { id: "text", label: t("services.probe.kindText") },
                  { id: "image", label: t("services.probe.kindImage") },
                ]}
                active={kind}
                disabled={busy}
                onSelect={(next) => {
                  setKindOverride(next as ProbeModelKind);
                  probe.reset();
                }}
              />
              <p className="text-caption text-content-muted">
                {t("services.probe.kindHint")}
              </p>
            </div>
          ) : null}

          <Field id="probe-prompt" label={t("services.probe.prompt")}>
            <Input
              id="probe-prompt"
              ref={promptRef}
              value={prompt}
              maxLength={MAX_PROBE_PROMPT_CHARS}
              disabled={busy}
              onChange={(event) => setPrompt(event.target.value)}
            />
          </Field>

          <div className="flex flex-wrap items-center gap-3">
            <Button
              disabled={!canSend}
              loading={probe.isPending}
              onClick={() =>
                probe.mutate({
                  subject,
                  request: { model: model.trim(), kind, prompt: prompt.trim() },
                })
              }
            >
              {probe.isPending ? null : (
                <Send className="h-4 w-4" aria-hidden="true" />
              )}
              {t(
                probe.isPending
                  ? "services.probe.sending"
                  : "services.probe.send",
              )}
            </Button>
            <p className="text-caption text-content-muted">
              {t("services.probe.cost")}
            </p>
          </div>

          {probe.error ? (
            <ProviderActionError
              title={t("services.probe.title", {
                name: subjectName(subject),
              })}
              error={probe.error}
              retryHint={t("services.test.retryHint")}
            />
          ) : null}

          {probe.data ? (
            <>
              <ProviderProbeResult outcome={probe.data} />
              {suggestedBaseUrl === null ? null : (
                <div className="flex flex-col gap-2 rounded-xl border border-hairline bg-layer-1 p-3">
                  <p className="text-caption text-content">
                    {t("services.probe.addressWorksInstead", {
                      url: suggestedBaseUrl,
                    })}
                  </p>
                  {provider !== null && canAdoptBaseUrl ? (
                    <Button
                      variant="secondary"
                      className="self-start"
                      loading={adoptBaseUrl.isPending}
                      onClick={() =>
                        adoptBaseUrl.mutate({
                          provider,
                          baseUrl: suggestedBaseUrl,
                        })
                      }
                    >
                      <Link2 className="h-4 w-4" aria-hidden="true" />
                      {t("services.probe.useAddress")}
                    </Button>
                  ) : null}
                  {adoptBaseUrl.isSuccess ? (
                    <p role="status" className="text-caption text-success">
                      {t("services.probe.addressAdopted")}
                    </p>
                  ) : null}
                  {adoptBaseUrl.error ? (
                    <p role="alert" className="text-caption text-danger">
                      {t("services.probe.addressAdoptFailed")}
                    </p>
                  ) : null}
                </div>
              )}
              {canAdopt && provider && profile.data ? (
                <Button
                  variant="secondary"
                  className="self-start"
                  loading={adopt.isPending}
                  onClick={() =>
                    adopt.mutate({
                      provider,
                      profile: profile.data,
                      model: model.trim(),
                    })
                  }
                >
                  <Radio className="h-4 w-4" aria-hidden="true" />
                  {t("services.probe.adopt")}
                </Button>
              ) : null}
              {adopt.isSuccess ? (
                <p role="status" className="text-caption text-success">
                  {t("services.probe.adopted")}
                </p>
              ) : null}
              {adopt.error ? (
                <p role="alert" className="text-caption text-danger">
                  {t("services.probe.adoptFailed")}
                </p>
              ) : null}
            </>
          ) : null}
        </div>
      )}
    </Modal>
  );
}
