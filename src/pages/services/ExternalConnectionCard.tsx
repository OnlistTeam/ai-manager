import { AlertTriangle, Globe2, KeyRound, PlugZap } from "lucide-react";
import { useTranslation } from "react-i18next";
import type {
  EffectiveConnection,
  ProviderRuntimeResource,
} from "@/entities/provider";
import type { ToolId } from "@/entities/tool";
import { Badge } from "@/shared/ui/Badge";
import { CopyButton } from "@/shared/ui/CopyButton";
import { ServiceCard } from "@/shared/ui/ServiceCard";
import { credentialCopy, describeSource } from "./effectiveConnectionCopy";
import { ServicesOpenConfigAction } from "./ServicesOpenConfigAction";

export interface ExternalConnectionCardProps {
  /** The effective connection already confirmed to have no matching endpoint in the list. */
  connection: EffectiveConnection;
  toolName: string;
  tool: ToolId;
  /** Which file this connection was read from, if any; when present, gives a direct entry to open it. */
  configResource?: ProviderRuntimeResource;
}

/** Host name as the title, full address in the detail — same reading pattern as a saved-endpoint card. */
function hostOf(endpoint: string): string {
  try {
    return new URL(endpoint).host || endpoint;
  } catch {
    return endpoint;
  }
}

/**
 * When the effective address isn't in the saved list, it's still a list
 * card, not a separate section opened above the list: every tool's layout
 * stays identical this way, and the list itself is the complete answer to
 * "who am I connected to right now".
 */
export function ExternalConnectionCard({
  connection,
  toolName,
  tool,
  configResource,
}: ExternalConnectionCardProps) {
  const { t } = useTranslation();
  const endpoint = connection.endpoint;
  if (endpoint === null) return null;

  const credentialMissing = connection.credential === "missing";

  return (
    <ServiceCard
      name={hostOf(endpoint)}
      icon={
        <span
          className="flex h-10 w-10 items-center justify-center rounded-lg bg-warning/15 text-warning"
          aria-hidden="true"
        >
          <PlugZap className="h-5 w-5" />
        </span>
      }
      usedBy={
        !connection.selection || connection.selection === "configuration"
          ? [toolName]
          : []
      }
      connected
      active={!connection.selection || connection.selection === "configuration"}
      useAvailable={false}
      activeLabelKey="services.card.inUse"
      notUsedLabelKey={`services.card.${connection.selection ?? "unknown"}`}
      unavailableLabelKey={
        connection.selection && connection.selection !== "configuration"
          ? "ds.action.readOnly"
          : undefined
      }
      compact
      actions={
        <ServicesOpenConfigAction tool={tool} resource={configResource} />
      }
      meta={
        <>
          <Badge tone="warning" icon={AlertTriangle}>
            {t("services.external.badge")}
          </Badge>
          {connection.selection && connection.selection !== "configuration" ? (
            <Badge tone="neutral">
              {t(`services.card.${connection.selection}`)}
            </Badge>
          ) : null}
        </>
      }
      detail={
        <div className="flex flex-col gap-2.5">
          <p className="flex min-w-0 items-start gap-2 text-content-muted">
            <Globe2
              className="mt-0.5 h-4 w-4 shrink-0 text-brand"
              aria-hidden="true"
            />
            <span className="min-w-0 flex-1">
              <span className="sr-only">{t("services.external.endpoint")}</span>
              <span className="block break-all font-mono text-mono-sm text-content">
                {endpoint}
              </span>
            </span>
            <CopyButton
              value={endpoint}
              label={t("services.external.endpoint")}
            />
          </p>
          <p className="text-caption text-content-muted">
            {describeSource(connection.endpointSource, t)}
          </p>
          <p className="flex items-center gap-2 text-content-muted">
            <KeyRound
              className={`h-4 w-4 shrink-0 ${
                credentialMissing ? "text-warning" : "text-content-muted"
              }`}
              aria-hidden="true"
            />
            <span className={credentialMissing ? "text-warning" : undefined}>
              {credentialCopy(connection.credential, toolName, t)}
            </span>
          </p>
        </div>
      }
    />
  );
}
