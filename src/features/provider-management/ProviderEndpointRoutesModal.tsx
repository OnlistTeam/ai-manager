import { CheckCircle2, Gauge, Plus, Trash2 } from "lucide-react";
import { useState, type KeyboardEvent } from "react";
import { useTranslation } from "react-i18next";
import { Button } from "@/shared/ui/Button";
import { Input } from "@/shared/ui/Input";
import { CopyButton } from "@/shared/ui/CopyButton";
import { Modal } from "@/shared/ui/Modal";
import { cn } from "@/shared/ui/cn";
import { normalizeProviderEndpoint } from "./providerEndpointRouteUtils";
import type { useProviderEndpointRoutes } from "./useProviderEndpointRoutes";

type EndpointForm = ReturnType<typeof useProviderEndpointRoutes>;

interface ProviderEndpointRoutesModalProps {
  open: boolean;
  disabled: boolean;
  form: EndpointForm;
  onOpenChange: (open: boolean) => void;
  onChange: () => void;
}

const validationKeys = {
  invalid: "services.form.routeInvalid",
  duplicate: "services.form.routeDuplicate",
  limit: "services.form.routeLimit",
  empty: "services.form.routeEmpty",
} as const;

/**
 * The service's alternate Base URLs and their speed test, one level below the
 * edit form, the way CC Switch keeps them behind "manage and test". Every
 * change here is part of the edit draft: Done only closes this dialog, and the
 * edit form's Save or Cancel decides whether any of it is kept.
 */
export function ProviderEndpointRoutesModal({
  open,
  disabled,
  form,
  onOpenChange,
  onChange,
}: ProviderEndpointRoutesModalProps) {
  const { t } = useTranslation();
  const [candidate, setCandidate] = useState("");
  const selected = normalizeProviderEndpoint(form.baseUrl);

  const add = () => {
    const added = form.addRoute(candidate);
    if (!added) return;
    setCandidate("");
    onChange();
  };
  const addOnEnter = (event: KeyboardEvent<HTMLInputElement>) => {
    if (event.key !== "Enter" || event.nativeEvent.isComposing) return;
    // The dialog is portalled out of the edit form's DOM but not out of its
    // React tree, so Enter would otherwise reach the form's submit handler.
    event.preventDefault();
    event.stopPropagation();
    add();
  };

  return (
    <Modal
      open={open}
      onOpenChange={onOpenChange}
      title={t("services.form.routes")}
      description={t("services.form.routesHint")}
      footer={
        <Button onClick={() => onOpenChange(false)}>
          {t("services.form.routesDone")}
        </Button>
      }
    >
      <div className="flex flex-col gap-3">
        <div className="flex items-center justify-between gap-3">
          <label className="flex items-center gap-2 text-caption text-content">
            <input
              type="checkbox"
              checked={form.autoSelect}
              disabled={disabled}
              className="h-4 w-4 rounded border-hairline accent-brand"
              onChange={(event) => {
                form.setAutoSelect(event.target.checked);
                onChange();
              }}
            />
            {t("services.form.routesAuto")}
          </label>
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
      </div>
    </Modal>
  );
}
