import { HardDrive, RefreshCw } from "lucide-react";
import { useTranslation } from "react-i18next";
import { toast } from "sonner";
import {
  useOpenProviderRuntimeResource,
  useProviderRuntimeContext,
  type ProviderRuntimeResource,
} from "@/entities/provider";
import type { ToolId } from "@/entities/tool";
import { toErrorCopy } from "@/shared/lib/nativeError";
import { Button } from "@/shared/ui/Button";
import { cn } from "@/shared/ui/cn";
import { DetectionStatus } from "@/shared/ui/DetectionStatus";
import { ProviderRuntimeResourceItem } from "./ProviderRuntimeResourceItem";
import { ProviderRuntimeStorageSummary } from "./ProviderRuntimeStorageSummary";

export interface ProviderRuntimeContextPanelProps {
  tool: ToolId | null;
  /** The tool's display name, so this panel names which tool it's showing instead of repeating the page's own heading. */
  toolName?: string | null;
  includeConfiguration?: boolean;
  /** The surrounding modal already supplies a heading and surface. */
  embedded?: boolean;
  /**
   * Given only when this tool's global instructions are managed by the
   * product's own Prompt library, which then owns editing them (ADR-0037).
   */
  onManageInstructions?: (() => void) | null;
}

/**
 * Copy still lives under `services.runtime.*` (the namespace wasn't renamed
 * when the component moved): `routing` / `usage` / `sessions` each kept
 * their top-level namespace even after being demoted to tabs — namespace
 * never equals location. Actually moving the keys would touch ~25
 * template-concatenated key names, and TS won't error on a missed one, only
 * exposing the bare key at runtime — not worth that risk just for directory
 * tidiness; recorded in ADR-0036.
 */
export function ProviderRuntimeContextPanel({
  tool,
  toolName,
  includeConfiguration = false,
  embedded = false,
  onManageInstructions = null,
}: ProviderRuntimeContextPanelProps) {
  const { t } = useTranslation();
  const query = useProviderRuntimeContext(tool);
  const openResource = useOpenProviderRuntimeResource({
    onSuccess: (outcome) => {
      toast.success(t(`services.runtime.openOutcome.${outcome}` as const));
      void query.refetch();
    },
    onError: (openError) => {
      const copy = toErrorCopy(openError);
      toast.error(t(copy.messageKey), {
        description: copy.remediationKey ? t(copy.remediationKey) : undefined,
      });
    },
  });
  const context = query.data;
  const error = query.error ? toErrorCopy(query.error) : null;

  if (tool === null) return null;

  function handleOpen(resource: ProviderRuntimeResource): void {
    if (tool === null) return;
    openResource.reset();
    openResource.mutate({ tool, resource: resource.id });
  }

  return (
    <section
      className={cn(
        "space-y-3",
        !embedded && "provider-runtime-panel overflow-hidden border p-4",
      )}
    >
      {!embedded ? (
        <div className="flex items-center gap-3">
          <span className="flex h-9 w-9 shrink-0 items-center justify-center rounded-lg bg-layer-1 text-brand">
            <HardDrive className="h-4 w-4" aria-hidden="true" />
          </span>
          <span className="min-w-0 flex-1">
            <span className="block text-body font-medium text-content">
              {toolName ?? t("services.runtime.title")}
            </span>
            <span className="mt-0.5 block text-caption text-content-muted">
              {t("services.runtime.description")}
            </span>
          </span>
        </div>
      ) : null}

      <div className="space-y-3">
        {context ? (
          <div className="flex flex-wrap items-start justify-between gap-3">
            <ProviderRuntimeStorageSummary storage={context.storage} />
            <Button
              variant="ghost"
              size="sm"
              loading={query.isFetching}
              onClick={() => void query.refetch()}
            >
              <RefreshCw className="h-4 w-4" aria-hidden="true" />
              {t("services.runtime.refresh")}
            </Button>
          </div>
        ) : null}

        {query.isPending && !context ? (
          <DetectionStatus
            label={t("services.runtime.loading")}
            className="min-h-20"
          />
        ) : null}

        {error ? (
          <div className="flex flex-wrap items-center justify-between gap-3 rounded-lg bg-danger/10 p-3">
            <div>
              <p className="text-body text-content">{t(error.messageKey)}</p>
              {error.remediationKey ? (
                <p className="mt-1 text-caption text-content-muted">
                  {t(error.remediationKey)}
                </p>
              ) : null}
            </div>
            <Button
              variant="secondary"
              disabled={query.isFetching}
              onClick={() => void query.refetch()}
            >
              <RefreshCw className="h-4 w-4" aria-hidden="true" />
              {t("ds.action.retry")}
            </Button>
          </div>
        ) : null}

        {context ? (
          <div className="space-y-3">
            <p className="text-caption leading-5 text-content-muted">
              {t("services.runtime.localOnlyNote")}
            </p>
            <ul className="divide-y divide-hairline rounded-lg border border-hairline px-4">
              {context.resources
                .filter(
                  (resource) =>
                    includeConfiguration || resource.kind !== "configuration",
                )
                .map((resource) => (
                  <ProviderRuntimeResourceItem
                    key={resource.id}
                    resource={resource}
                    busy={
                      openResource.isPending &&
                      openResource.variables?.resource === resource.id
                    }
                    onOpen={() => handleOpen(resource)}
                    onManage={
                      resource.kind === "instructions"
                        ? onManageInstructions
                        : null
                    }
                  />
                ))}
            </ul>
            <p className="text-caption leading-5 text-content-muted">
              {t("services.runtime.projectUsageNote")}
            </p>
          </div>
        ) : null}
      </div>
    </section>
  );
}
