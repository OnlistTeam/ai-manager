import { AlertCircle, Network } from "lucide-react";
import { useEffect, useId, useState } from "react";
import { useTranslation } from "react-i18next";
import {
  useNetworkProxy,
  useSaveNetworkProxy,
  type NetworkProxyMode,
  type NetworkProxySettings,
} from "@/entities/network-proxy";
import { toErrorCopy } from "@/shared/lib/nativeError";
import { Button } from "@/shared/ui/Button";
import { Card } from "@/shared/ui/Card";
import { DetectionStatus } from "@/shared/ui/DetectionStatus";
import { Input } from "@/shared/ui/Input";
import { PreferenceChoiceGroup } from "./PreferenceChoiceGroup";

type Translate = ReturnType<typeof useTranslation>["t"];

/** One line saying where the app's requests go now. */
function statusLine(settings: NetworkProxySettings, t: Translate): string {
  if (settings.protected) return t("preferences.network.proxy.protectedHint");
  const proxy = settings.inUse ?? "";
  switch (settings.source) {
    case "custom":
      return t("preferences.network.proxy.status.custom", { proxy });
    case "environment":
      return t("preferences.network.proxy.status.environment", { proxy });
    case "system":
      return t("preferences.network.proxy.status.system", { proxy });
    case "none":
      return t("preferences.network.proxy.status.none");
    case "off":
      return t("preferences.network.proxy.status.off");
  }
}

/**
 * Which proxy the app's own requests use (ADR-0056): model lists and tests,
 * downloads, updates and forwarded requests. Following the system is the
 * default; "off" goes direct even when the system names a proxy; a custom
 * address is a loopback one without credentials.
 */
export function NetworkProxyCard() {
  const { t } = useTranslation();
  const fieldId = useId();
  const statusId = `${fieldId}-status`;
  const query = useNetworkProxy();
  const save = useSaveNetworkProxy();
  const [draft, setDraft] = useState("");
  // Custom picked with nothing saved yet; the saved mode is the truth after.
  const [composing, setComposing] = useState(false);
  const error = save.error ? toErrorCopy(save.error) : null;

  useEffect(() => {
    if (query.data?.url !== undefined) setDraft(query.data.url ?? "");
  }, [query.data?.url]);

  const settings = query.data;
  const mode: NetworkProxyMode = composing
    ? "custom"
    : (settings?.mode ?? "auto");
  const trimmed = draft.trim();
  const unchanged =
    settings?.mode === "custom" &&
    !settings.protected &&
    trimmed === (settings.url ?? "");

  function choose(next: NetworkProxyMode): void {
    save.reset();
    if (next === "custom") {
      setComposing(settings?.mode !== "custom");
      return;
    }
    setComposing(false);
    if (next !== settings?.mode) save.mutate({ mode: next, url: null });
  }

  function saveCustom(): void {
    if (trimmed.length === 0 || unchanged) return;
    save.mutate(
      { mode: "custom", url: trimmed },
      { onSuccess: () => setComposing(false) },
    );
  }

  return (
    <section aria-labelledby={`${fieldId}-title`}>
      <Card padding="none" className="overflow-hidden rounded-xl px-4">
        <div className="grid gap-3 py-3 sm:grid-cols-[minmax(0,1fr)_auto] sm:items-center">
          <div className="flex min-w-0 items-start gap-3">
            <span className="flex h-9 w-9 shrink-0 items-center justify-center rounded-lg bg-layer-1 text-brand">
              <Network className="h-4 w-4" aria-hidden="true" />
            </span>
            <div className="min-w-0">
              <h3
                id={`${fieldId}-title`}
                title={t("preferences.network.proxy.scope")}
                className="text-body font-medium text-content"
              >
                {t("preferences.network.proxy.title")}
              </h3>
              <p
                id={statusId}
                className="mt-0.5 text-caption text-content-muted"
              >
                {query.isPending
                  ? null
                  : settings
                    ? statusLine(settings, t)
                    : t("preferences.network.proxy.readError")}
              </p>
            </div>
          </div>
          {settings ? (
            <PreferenceChoiceGroup
              label={t("preferences.network.proxy.title")}
              descriptionId={statusId}
              value={mode}
              disabled={save.isPending}
              className="w-full sm:w-72"
              onChange={choose}
              options={[
                { value: "auto", label: t("preferences.network.proxy.auto") },
                { value: "off", label: t("preferences.network.proxy.off") },
                {
                  value: "custom",
                  label: t("preferences.network.proxy.custom"),
                },
              ]}
            />
          ) : query.isError ? (
            <Button
              size="sm"
              variant="secondary"
              onClick={() => void query.refetch()}
            >
              {t("preferences.retry")}
            </Button>
          ) : null}
        </div>

        {query.isPending ? (
          <DetectionStatus
            className="mb-3"
            label={t("preferences.network.proxy.loading")}
          />
        ) : null}

        {settings && mode === "custom" ? (
          <div className="border-t border-hairline py-3">
            <div className="flex items-center gap-2">
              <Input
                id={fieldId}
                aria-label={t("preferences.network.proxy.label")}
                value={draft}
                disabled={save.isPending}
                invalid={Boolean(error)}
                spellCheck={false}
                autoCapitalize="off"
                autoComplete="off"
                placeholder="http://127.0.0.1:7890"
                className="min-w-0 flex-1"
                onChange={(event) => {
                  save.reset();
                  setDraft(event.target.value);
                }}
                onKeyDown={(event) => {
                  if (event.key === "Enter") saveCustom();
                }}
              />
              <Button
                size="sm"
                variant="secondary"
                loading={save.isPending}
                disabled={trimmed.length === 0 || unchanged}
                onClick={saveCustom}
              >
                {t("preferences.network.proxy.save")}
              </Button>
            </div>
          </div>
        ) : null}

        {error ? (
          <p
            role="alert"
            className="flex items-center gap-2 pb-3 text-caption text-danger"
          >
            <AlertCircle className="h-3.5 w-3.5" aria-hidden="true" />
            {t(error.messageKey)}
          </p>
        ) : null}
      </Card>
    </section>
  );
}
