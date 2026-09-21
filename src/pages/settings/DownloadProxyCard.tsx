import { AlertCircle, CheckCircle2, Network, ShieldCheck } from "lucide-react";
import { useEffect, useId, useState } from "react";
import { useTranslation } from "react-i18next";
import { useNetworkProxy, useSaveNetworkProxy } from "@/entities/network-proxy";
import { toErrorCopy } from "@/shared/lib/nativeError";
import { Badge } from "@/shared/ui/Badge";
import { Button } from "@/shared/ui/Button";
import { Card } from "@/shared/ui/Card";
import { DetectionStatus } from "@/shared/ui/DetectionStatus";
import { Field } from "@/shared/ui/Field";
import { Input } from "@/shared/ui/Input";
import { Switch } from "@/shared/ui/Switch";

export function DownloadProxyCard() {
  const { t } = useTranslation();
  const fieldId = useId();
  const descriptionId = `${fieldId}-description`;
  const query = useNetworkProxy();
  const save = useSaveNetworkProxy();
  const [draft, setDraft] = useState("");
  /*
   * The vast majority of people never need a proxy: official downloads work
   * fine, and registry mirrors cover the rest. So this card defaults to a
   * single toggle row — the address field only appears once someone actually
   * opens it.
   *
   * Local state only covers the brief "want to configure but haven't saved
   * yet" window. Once saved, `configured` is the single source of truth —
   * keeping a local copy in sync with it would only create a discrepancy
   * between the two if the save fails.
   */
  const [composing, setComposing] = useState(false);
  const error = save.error ? toErrorCopy(save.error) : null;

  useEffect(() => {
    if (query.data?.url !== undefined) setDraft(query.data.url ?? "");
  }, [query.data?.url]);

  const configured = query.data?.configured ?? false;
  const enabled = configured || composing;
  const trimmed = draft.trim();
  const unchanged =
    !query.data?.protected && trimmed === (query.data?.url ?? "");

  /* Turning off the switch already means "switch to a direct connection" — no need for another button to say it again. */
  function toggle(next: boolean): void {
    save.reset();
    setComposing(next);
    if (!next && configured) save.mutate(null);
  }

  return (
    <section aria-labelledby={`${fieldId}-title`}>
      <Card padding="lg" className="rounded-xl">
        <div className="flex items-start gap-4">
          <span className="flex h-10 w-10 shrink-0 items-center justify-center rounded-xl bg-brand/10">
            <Network className="h-5 w-5 text-brand" aria-hidden="true" />
          </span>
          <div className="min-w-0 flex-1">
            <div className="flex flex-wrap items-center gap-2">
              <h3 id={`${fieldId}-title`} className="text-heading text-content">
                {t("preferences.network.proxy.title")}
              </h3>
              {query.data?.configured ? (
                <Badge
                  tone={query.data.protected ? "warning" : "success"}
                  icon={query.data.protected ? ShieldCheck : CheckCircle2}
                >
                  {t(
                    query.data.protected
                      ? "preferences.network.proxy.protected"
                      : "preferences.network.proxy.active",
                  )}
                </Badge>
              ) : null}
            </div>
            <p
              id={descriptionId}
              className="mt-1 text-caption leading-5 text-content-muted"
            >
              {t("preferences.network.proxy.description")}
            </p>
          </div>
          {query.isPending || query.isError ? null : (
            <Switch
              checked={enabled}
              disabled={save.isPending}
              aria-label={t("preferences.network.proxy.title")}
              aria-describedby={descriptionId}
              onCheckedChange={toggle}
            />
          )}
        </div>

        {query.isPending ? (
          <DetectionStatus
            className="mt-5 min-h-20"
            label={t("preferences.network.proxy.loading")}
          />
        ) : query.isError ? (
          <div
            role="alert"
            className="mt-5 flex items-center gap-3 text-caption text-danger"
          >
            <AlertCircle className="h-4 w-4" aria-hidden="true" />
            <span className="min-w-0 flex-1">
              {t("preferences.network.proxy.readError")}
            </span>
            <Button
              size="sm"
              variant="secondary"
              onClick={() => void query.refetch()}
            >
              {t("preferences.retry")}
            </Button>
          </div>
        ) : enabled ? (
          <div className="mt-5">
            {query.data?.protected ? (
              <p className="mb-3 rounded-lg border border-warning/25 bg-warning/5 px-3 py-2 text-caption text-content-muted">
                {t("preferences.network.proxy.protectedHint")}
              </p>
            ) : null}
            <Field
              id={fieldId}
              label={t("preferences.network.proxy.label")}
              hint={t("preferences.network.proxy.hint")}
              error={error ? t(error.messageKey) : undefined}
            >
              <Input
                id={fieldId}
                value={draft}
                disabled={save.isPending}
                invalid={Boolean(error)}
                spellCheck={false}
                autoCapitalize="off"
                autoComplete="off"
                placeholder="http://127.0.0.1:7890"
                onChange={(event) => {
                  save.reset();
                  setDraft(event.target.value);
                }}
              />
            </Field>
            <div className="mt-4 flex justify-end">
              <Button
                size="sm"
                variant="secondary"
                loading={save.isPending}
                disabled={trimmed.length === 0 || unchanged}
                onClick={() => save.mutate(trimmed)}
              >
                {t("preferences.network.proxy.save")}
              </Button>
            </div>
          </div>
        ) : null}
      </Card>
    </section>
  );
}
