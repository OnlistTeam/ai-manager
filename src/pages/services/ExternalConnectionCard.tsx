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
  EffectiveConnectionSource,
  ProviderRuntimeResource,
} from "@/entities/provider";
import type { ToolId } from "@/entities/tool";
import { Badge } from "@/shared/ui/Badge";
import { Button } from "@/shared/ui/Button";
import { CopyButton } from "@/shared/ui/CopyButton";
import { ServiceCard } from "@/shared/ui/ServiceCard";
import {
  credentialCopy,
  describeSource,
  sameSource,
} from "./effectiveConnectionCopy";
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
   * Opens the edit dialog for one start-up line. Absent when nothing on this
   * card could be pinned down, which is the honest state for a variable
   * something set in a way this product does not model: guessing at which line
   * to rewrite in someone's shell profile is not an option.
   */
  onEditVariable?: (variable: string) => void;
  /**
   * The variables whose lines were located and are safe to rewrite.
   *
   * A set rather than one name: the address and the key are usually two
   * different variables on two different lines, and offering a single "Edit"
   * that silently picked whichever came first meant the key could never be
   * changed from here.
   */
  editableVariables?: ReadonlySet<string>;
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
  editableVariables,
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

  /**
   * The edit button for one row, or nothing.
   *
   * It sits beside the value it changes rather than in the action bar, because
   * this card shows two values from two different lines and a button in the
   * bar could not say which one it meant.
   */
  const editActionFor = (source: EffectiveConnectionSource) => {
    if (source.kind !== "environment" && source.kind !== "shellFile") {
      return null;
    }
    if (
      onEditVariable === undefined ||
      !editableVariables?.has(source.variable)
    ) {
      return null;
    }
    const variable = source.variable;
    return (
      <Button
        size="sm"
        variant="ghost"
        className="shrink-0"
        disabled={actionsBlocked}
        aria-label={t("services.shellVariable.editNamed", { variable })}
        onClick={() => onEditVariable(variable)}
      >
        <FileCog className="h-4 w-4" aria-hidden="true" />
        <span className="sr-only">{t("services.action.edit")}</span>
      </Button>
    );
  };

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
            {editActionFor(connection.endpointSource)}
          </p>
          <p className="text-caption text-content-muted">
            {describeSource(connection.endpointSource, t)}
          </p>
          <p className="flex min-w-0 items-center gap-2 text-content-muted">
            <KeyRound
              className={`h-4 w-4 shrink-0 ${
                credentialMissing ? "text-warning" : "text-content-muted"
              }`}
              aria-hidden="true"
            />
            <span
              className={`min-w-0 flex-1 ${credentialMissing ? "text-warning" : ""}`}
            >
              {credentialCopy(connection.credential, toolName, t)}
            </span>
            {editActionFor(connection.credentialSource)}
          </p>
          {/* Where the key comes from, on the same footing as the address —
              but only when that is somewhere else. The two usually share one
              file, and repeating the identical sentence says nothing. */}
          {connection.credentialSource.kind !== "toolDefault" &&
          !sameSource(
            connection.credentialSource,
            connection.endpointSource,
          ) ? (
            <p className="text-caption text-content-muted">
              {describeSource(connection.credentialSource, t)}
            </p>
          ) : null}
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
