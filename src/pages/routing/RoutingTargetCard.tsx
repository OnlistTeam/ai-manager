import { CheckCircle2, CircleAlert, GitBranch, Shield } from "lucide-react";
import { useTranslation } from "react-i18next";
import type { RoutingTarget } from "@/entities/routing";
import { Badge } from "@/shared/ui/Badge";
import { Card } from "@/shared/ui/Card";
import { Switch } from "@/shared/ui/Switch";
import { ToolArtwork } from "@/shared/ui/ToolArtwork";
import { RoutingProviderQueue } from "./RoutingProviderQueue";

interface RoutingTargetCardProps {
  target: RoutingTarget;
  busy: boolean;
  onTakeoverChange: (enabled: boolean) => void;
  onFailoverChange: (enabled: boolean) => void;
  onAdd: (providerId: string) => void;
  onRemove: (providerId: string) => void;
  onSwitch: (providerId: string) => void;
}

export function RoutingTargetCard({
  target,
  busy,
  onTakeoverChange,
  onFailoverChange,
  onAdd,
  onRemove,
  onSwitch,
}: RoutingTargetCardProps) {
  const { t } = useTranslation();
  const name = t(`routing.tool.${target.tool}`);
  const current = target.currentProvider;

  return (
    <Card
      padding="lg"
      className="rounded-xl border-hairline bg-layer-1 shadow-sm"
      data-routing-target={target.tool}
    >
      <div className="flex items-start gap-4">
        <ToolArtwork toolId={target.tool} />
        <div className="min-w-0 flex-1">
          <div className="flex flex-wrap items-center gap-2">
            <h2 className="text-heading text-content">{name}</h2>
            <Badge
              tone={target.takeoverEnabled ? "success" : "neutral"}
              icon={target.takeoverEnabled ? CheckCircle2 : Shield}
            >
              {t(
                target.takeoverEnabled
                  ? "routing.target.routed"
                  : "routing.target.direct",
              )}
            </Badge>
          </div>
          <p className="mt-1 truncate text-caption text-content-muted">
            {current
              ? t("routing.target.current", { name: current.name })
              : t("routing.target.noProvider")}
          </p>
        </div>
      </div>

      <div className="mt-5 space-y-3">
        <div className="flex items-center gap-4 rounded-lg border border-hairline bg-layer-1 p-3">
          <Shield className="h-4 w-4 shrink-0 text-brand" aria-hidden="true" />
          <div className="min-w-0 flex-1">
            <p className="text-body font-medium text-content">
              {t("routing.takeover.title")}
            </p>
            <p className="mt-0.5 text-caption text-content-muted">
              {t("routing.takeover.description")}
            </p>
          </div>
          <Switch
            checked={target.takeoverEnabled}
            disabled={busy || current === null}
            aria-label={t("routing.takeover.label", { name })}
            onCheckedChange={onTakeoverChange}
          />
        </div>

        <div className="flex items-center gap-4 rounded-lg border border-hairline bg-layer-1 p-3">
          {target.autoFailoverEnabled ? (
            <GitBranch
              className="h-4 w-4 shrink-0 text-success"
              aria-hidden="true"
            />
          ) : (
            <CircleAlert
              className="h-4 w-4 shrink-0 text-content-muted"
              aria-hidden="true"
            />
          )}
          <div className="min-w-0 flex-1">
            <p className="text-body font-medium text-content">
              {t("routing.failover.title")}
            </p>
            <p className="mt-0.5 text-caption text-content-muted">
              {t("routing.failover.description")}
            </p>
          </div>
          <Switch
            checked={target.autoFailoverEnabled}
            disabled={busy || !target.takeoverEnabled}
            aria-label={t("routing.failover.label", { name })}
            onCheckedChange={onFailoverChange}
          />
        </div>
      </div>

      <RoutingProviderQueue
        target={target}
        busy={busy}
        onAdd={onAdd}
        onRemove={onRemove}
        onSwitch={onSwitch}
      />
    </Card>
  );
}
