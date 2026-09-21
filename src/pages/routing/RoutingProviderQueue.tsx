import { ArrowRight, CheckCircle2, CircleAlert, Trash2 } from "lucide-react";
import { useState } from "react";
import { useTranslation } from "react-i18next";
import type { RoutingTarget } from "@/entities/routing";
import { Badge } from "@/shared/ui/Badge";
import { Button } from "@/shared/ui/Button";

interface RoutingProviderQueueProps {
  target: RoutingTarget;
  busy: boolean;
  onAdd: (providerId: string) => void;
  onRemove: (providerId: string) => void;
  onSwitch: (providerId: string) => void;
}

export function RoutingProviderQueue({
  target,
  busy,
  onAdd,
  onRemove,
  onSwitch,
}: RoutingProviderQueueProps) {
  const { t } = useTranslation();
  const [selected, setSelected] = useState("");
  const canAdd = selected.length > 0 && !busy;
  const targetName = t(`routing.tool.${target.tool}`);

  return (
    <div className="mt-5 border-t border-hairline pt-4">
      <div className="flex items-center justify-between gap-3">
        <div>
          <h3 className="text-body font-medium text-content">
            {t("routing.queue.title")}
          </h3>
          <p className="mt-0.5 text-caption text-content-muted">
            {t("routing.queue.description")}
          </p>
        </div>
        <Badge tone={target.queue.length > 0 ? "brand" : "neutral"}>
          {t("routing.queue.count", { count: target.queue.length })}
        </Badge>
      </div>

      {target.queue.length > 0 ? (
        <ol className="mt-3 space-y-2" aria-label={t("routing.queue.title")}>
          {target.queue.map((provider) => (
            <li
              key={provider.id}
              className="flex min-w-0 items-center gap-3 rounded-lg border border-hairline bg-layer-1 px-3 py-2.5"
            >
              <span className="flex h-7 w-7 shrink-0 items-center justify-center rounded-full bg-brand/10 text-caption font-semibold text-content">
                P{provider.priority}
              </span>
              <span className="min-w-0 flex-1">
                <span className="flex items-center gap-2">
                  <span className="truncate text-body font-medium text-content">
                    {provider.name}
                  </span>
                  {provider.current ? (
                    <Badge tone="success" icon={CheckCircle2}>
                      {t("routing.provider.current")}
                    </Badge>
                  ) : null}
                </span>
                <span className="mt-0.5 flex items-center gap-1.5 text-caption text-content-muted">
                  {provider.healthy ? (
                    <CheckCircle2
                      className="h-3.5 w-3.5 text-success"
                      aria-hidden="true"
                    />
                  ) : (
                    <CircleAlert
                      className="h-3.5 w-3.5 text-danger"
                      aria-hidden="true"
                    />
                  )}
                  {t(
                    provider.healthy
                      ? "routing.provider.healthy"
                      : "routing.provider.unhealthy",
                    { count: provider.consecutiveFailures },
                  )}
                </span>
              </span>
              {!provider.current && target.takeoverEnabled ? (
                <Button
                  size="sm"
                  variant="ghost"
                  disabled={busy}
                  aria-label={t("routing.provider.switchLabel", {
                    name: provider.name,
                  })}
                  onClick={() => onSwitch(provider.id)}
                >
                  <ArrowRight className="h-4 w-4" aria-hidden="true" />
                  {t("routing.provider.switch")}
                </Button>
              ) : null}
              <Button
                size="sm"
                variant="ghost"
                disabled={busy}
                aria-label={t("routing.provider.removeLabel", {
                  name: provider.name,
                })}
                onClick={() => onRemove(provider.id)}
              >
                <Trash2 className="h-4 w-4" aria-hidden="true" />
              </Button>
            </li>
          ))}
        </ol>
      ) : (
        <p className="mt-3 rounded-lg border border-dashed border-hairline px-3 py-3 text-caption text-content-muted">
          {t("routing.queue.empty")}
        </p>
      )}

      {target.available.length > 0 ? (
        <div className="mt-3 flex gap-2">
          <select
            value={selected}
            disabled={busy}
            aria-label={t("routing.queue.selectLabelNamed", {
              name: targetName,
            })}
            onChange={(event) => setSelected(event.target.value)}
            className="min-w-0 flex-1 disabled:opacity-50"
          >
            <option value="">{t("routing.queue.selectPlaceholder")}</option>
            {target.available.map((provider) => (
              <option key={provider.id} value={provider.id}>
                {provider.name}
              </option>
            ))}
          </select>
          <Button
            size="sm"
            variant="secondary"
            disabled={!canAdd}
            aria-label={t("routing.queue.addNamed", { name: targetName })}
            onClick={() => {
              onAdd(selected);
              setSelected("");
            }}
          >
            {t("routing.queue.add")}
          </Button>
        </div>
      ) : null}
    </div>
  );
}
