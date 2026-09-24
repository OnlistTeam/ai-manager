import { CheckCircle2, Gauge, Plus, Trash2 } from "lucide-react";
import { useState, type KeyboardEvent } from "react";
import { useTranslation } from "react-i18next";
import { Button } from "@/shared/ui/Button";
import { Field } from "@/shared/ui/Field";
import { Input } from "@/shared/ui/Input";
import { CopyableInput } from "@/shared/ui/CopyableInput";
import { CopyButton } from "@/shared/ui/CopyButton";
import { cn } from "@/shared/ui/cn";
import {
  normalizeProviderEndpoint,
  trailingVersionSegment,
} from "./providerEndpointRouteUtils";
import type { useProviderEndpointRoutes } from "./useProviderEndpointRoutes";

type EndpointForm = ReturnType<typeof useProviderEndpointRoutes>;

interface ProviderEndpointFieldsProps {
  disabled: boolean;
  /** `ProviderEditProfile.baseUrlTakesNoVersion`. */
  takesNoVersion: boolean;
  form: EndpointForm;
  onChange: () => void;
}

const validationKeys = {
  invalid: "services.form.routeInvalid",
  duplicate: "services.form.routeDuplicate",
  limit: "services.form.routeLimit",
  empty: "services.form.routeEmpty",
} as const;

export function ProviderEndpointFields({
  disabled,
  takesNoVersion,
  form,
  onChange,
}: ProviderEndpointFieldsProps) {
  const { t } = useTranslation();
  const [candidate, setCandidate] = useState("");
  const selected = normalizeProviderEndpoint(form.baseUrl);
  const versionSegment = takesNoVersion
    ? trailingVersionSegment(form.baseUrl)
    : null;
  const baseUrlWarning =
    versionSegment === null
      ? undefined
      : t("services.form.baseUrlVersionDoubled", { segment: versionSegment });

  const add = () => {
    const added = form.addRoute(candidate);
    if (!added) return;
    setCandidate("");
    onChange();
  };
  const addOnEnter = (event: KeyboardEvent<HTMLInputElement>) => {
    if (event.key !== "Enter" || event.nativeEvent.isComposing) return;
    event.preventDefault();
    event.stopPropagation();
    add();
  };

  return (
    <div className="flex flex-col gap-4">
      <Field
        id="service-base-url"
        label={t("services.form.baseUrl")}
        hint={baseUrlWarning ? undefined : t("services.form.baseUrlHint")}
        warning={baseUrlWarning}
      >
        <CopyableInput
          id="service-base-url"
          copyLabel={t("services.form.baseUrl")}
          type="url"
          value={form.baseUrl}
          maxLength={2_048}
          spellCheck={false}
          disabled={disabled}
          invalid={form.validation === "invalid"}
          className="font-mono"
          onChange={(event) => {
            form.setBaseUrl(event.target.value);
            onChange();
          }}
        />
      </Field>

      <details className="group rounded-xl border border-hairline bg-layer-1 p-3">
        <summary className="cursor-pointer list-none text-caption font-medium text-content">
          <span id="service-routes-title">{t("services.form.routes")}</span>
          <span className="ml-2 font-normal text-content-muted">
            {t("services.form.routesHint")}
          </span>
        </summary>

        <section
          className="mt-3 flex flex-col gap-2.5 border-t border-hairline pt-3"
          aria-labelledby="service-routes-title"
        >
          <div className="flex justify-end">
            <Button
              variant="secondary"
              size="sm"
              loading={form.testing}
              disabled={disabled || form.routes.length === 0}
              onClick={() => {
                form.runTest();
                onChange();
              }}
            >
              <Gauge className="h-4 w-4" aria-hidden="true" />
              {t(
                form.testing
                  ? "services.form.routesTesting"
                  : "services.form.routesTest",
              )}
            </Button>
          </div>

          <label className="flex items-start gap-2 text-caption text-content">
            <input
              type="checkbox"
              checked={form.autoSelect}
              disabled={disabled}
              className="mt-0.5 h-4 w-4 rounded border-hairline accent-brand"
              onChange={(event) => {
                form.setAutoSelect(event.target.checked);
                onChange();
              }}
            />
            <span>
              <span className="block">{t("services.form.routesAuto")}</span>
              <span className="block text-content-muted">
                {t("services.form.routesAutoHint")}
              </span>
            </span>
          </label>

          <div className="flex min-w-0 gap-2">
            <Input
              value={candidate}
              type="url"
              maxLength={2_048}
              spellCheck={false}
              disabled={disabled}
              aria-label={t("services.form.routeAddLabel")}
              placeholder={t("services.form.routeAddPlaceholder")}
              className="min-w-0 font-mono"
              onChange={(event) => setCandidate(event.target.value)}
              onKeyDown={addOnEnter}
            />
            <Button
              variant="secondary"
              size="sm"
              disabled={disabled}
              aria-label={t("services.form.routeAdd")}
              onClick={add}
            >
              <Plus className="h-4 w-4" aria-hidden="true" />
              {t("services.form.routeAdd")}
            </Button>
          </div>

          {form.validation ? (
            <p role="alert" className="text-caption text-danger">
              {t(validationKeys[form.validation])}
            </p>
          ) : null}
          {form.testFailed ? (
            <p role="alert" className="text-caption text-danger">
              {t("services.form.routesTestFailed")}
            </p>
          ) : null}

          {form.routes.length === 0 ? (
            <p className="rounded-lg border border-dashed border-hairline p-3 text-caption text-content-muted">
              {t("services.form.routeEmpty")}
            </p>
          ) : (
            <ul className="flex flex-col gap-1.5">
              {form.routes.map((route) => {
                const active = route.url === selected;
                const measurement = form.measurements.get(route.id);
                return (
                  <li
                    key={route.id}
                    className={cn(
                      "flex min-w-0 items-center gap-2 rounded-lg border bg-layer-1 p-2",
                      active ? "border-brand/45" : "border-hairline",
                    )}
                  >
                    <button
                      type="button"
                      aria-pressed={active}
                      disabled={disabled}
                      className="flex min-w-0 flex-1 items-center gap-2 text-left text-caption text-content disabled:opacity-60"
                      onClick={() => {
                        form.selectRoute(route);
                        onChange();
                      }}
                    >
                      <CheckCircle2
                        className={cn(
                          "h-4 w-4 shrink-0",
                          active ? "text-brand" : "text-content-muted/45",
                        )}
                        aria-hidden="true"
                      />
                      <span className="min-w-0 flex-1 break-all font-mono">
                        {route.url}
                      </span>
                      <span className="flex shrink-0 items-center gap-1.5 tabular-nums text-content-muted">
                        {active ? (
                          <span>{t("services.form.routeFixed")}</span>
                        ) : null}
                        {measurement?.failure === null &&
                        measurement.latencyMs !== null ? (
                          <span>
                            {t("services.form.routeLatency", {
                              latency: measurement.latencyMs,
                            })}
                          </span>
                        ) : measurement ? (
                          <span>{t("services.form.routeUnreachable")}</span>
                        ) : null}
                      </span>
                    </button>
                    <CopyButton value={route.url} label={route.url} />
                    <Button
                      variant="ghost"
                      size="sm"
                      disabled={disabled || active}
                      aria-label={t("services.form.routeRemove", {
                        route: route.url,
                      })}
                      onClick={() => {
                        form.removeRoute(route.id);
                        onChange();
                      }}
                    >
                      <Trash2 className="h-4 w-4" aria-hidden="true" />
                    </Button>
                  </li>
                );
              })}
            </ul>
          )}
        </section>
      </details>
    </div>
  );
}
