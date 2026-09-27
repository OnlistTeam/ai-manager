import { GitBranch } from "lucide-react";
import { useTranslation } from "react-i18next";
import type { RoutingTarget } from "@/entities/routing";
import { Switch } from "@/shared/ui/Switch";
import { RoutingProviderQueue } from "./RoutingProviderQueue";

export interface RoutingFailoverProps {
  target: RoutingTarget;
  busy: boolean;
  onFailoverChange: (enabled: boolean) => void;
  onAdd: (providerId: string) => void;
  onRemove: (providerId: string) => void;
  onSwitch: (providerId: string) => void;
}

/**
 * Under a routed tool: whether the gateway moves on to a backup endpoint when
 * one fails, and in which order.
 */
export function RoutingFailover({
  target,
  busy,
  onFailoverChange,
  onAdd,
  onRemove,
  onSwitch,
}: RoutingFailoverProps) {
  const { t } = useTranslation();
  const name = t(`routing.tool.${target.tool}`);

  return (
    <div className="ml-10 flex flex-col gap-2 border-l border-hairline pl-3">
      <div className="flex items-center gap-3">
        <GitBranch
          className={
            target.autoFailoverEnabled
              ? "h-3.5 w-3.5 shrink-0 text-success"
              : "h-3.5 w-3.5 shrink-0 text-content-muted"
          }
          aria-hidden="true"
        />
        <div className="min-w-0 flex-1">
          <p className="text-caption font-medium text-content">
            {t("routing.failover.title")}
          </p>
          <p className="text-caption text-content-muted">
            {t("routing.failover.description")}
          </p>
        </div>
        <Switch
          checked={target.autoFailoverEnabled}
          disabled={busy}
          aria-label={t("routing.failover.label", { name })}
          onCheckedChange={onFailoverChange}
        />
      </div>
      {target.autoFailoverEnabled || target.queue.length > 0 ? (
        <RoutingProviderQueue
          target={target}
          busy={busy}
          onAdd={onAdd}
          onRemove={onRemove}
          onSwitch={onSwitch}
        />
      ) : null}
    </div>
  );
}
