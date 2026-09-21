import {
  AlertTriangle,
  FileCog,
  Globe2,
  KeyRound,
  PlugZap,
  Radio,
} from "lucide-react";
import { useTranslation } from "react-i18next";
import type {
  EffectiveConnection,
  ProviderRuntimeResource,
} from "@/entities/provider";
import type { ToolId } from "@/entities/tool";
import { Badge } from "@/shared/ui/Badge";
import { Button } from "@/shared/ui/Button";
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
  /** Opens the test dialog. Absent when this tool cannot be tested at all. */
  onTest?: () => void;
  /**
   * Opens the edit dialog for the start-up line behind this connection. Absent
   * when no line could be pinned down, which is the honest state for a
   * variable something set in a way this product does not model: guessing at
   * which line to rewrite in someone's shell profile is not an option.
   */
  onEditVariable?: () => void;
  /** The variable whose line the edit button would change, for its label. */
  editVariableName?: string;
  actionsBlocked?: boolean;
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
  onTest,
  onEditVariable,
  editVariableName,
  actionsBlocked = false,
}: ExternalConnectionCardProps) {
  const { t } = useTranslation();
  const endpoint = connection.endpoint;
  if (endpoint === null) return null;

  const credentialMissing = connection.credential === "missing";
  // A credential this backend cannot read back — a tool's own OAuth login — has
  // nothing to send as a bearer token, so no test could succeed and no button
  // is offered. Everything else is testable, including an endpoint with no key.
  const testable =
    onTest !== undefined &&
    (connection.credential === "configured" ||
      connection.credential === "missing");

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
        <>
          {testable ? (
            <Button
              variant="ghost"
              disabled={actionsBlocked}
              aria-label={t("services.action.testNamed", {
                name: hostOf(endpoint),
              })}
              onClick={onTest}
            >
              <Radio className="h-4 w-4" aria-hidden="true" />
              {t("services.action.test")}
            </Button>
          ) : null}
          {onEditVariable && editVariableName ? (
            <Button
              variant="ghost"
              disabled={actionsBlocked}
              aria-label={t("services.shellVariable.editNamed", {
                variable: editVariableName,
              })}
              onClick={onEditVariable}
            >
              <FileCog className="h-4 w-4" aria-hidden="true" />
              {t("services.action.edit")}
            </Button>
          ) : null}
          <ServicesOpenConfigAction tool={tool} resource={configResource} />
        </>
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
          {/* Replaces the old "not saved here" wording: what matters is not
              where the record lives but that this address wins over anything
              chosen in the list below. */}
          <p className="text-caption text-content-muted">
            {t("services.external.overrides", { name: toolName })}
          </p>
        </div>
      }
    />
  );
}
