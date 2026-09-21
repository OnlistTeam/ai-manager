import {
  AlertTriangle,
  CheckCircle2,
  ChevronDown,
  Layers3,
  MapPin,
  TerminalSquare,
} from "lucide-react";
import { useTranslation } from "react-i18next";
import type { Tool, ToolUpdateMethod, ToolUpdatePreview } from "@/native";
import { Badge } from "@/shared/ui/Badge";
import { ToolArtwork } from "@/shared/ui/ToolArtwork";

export interface UpdatePreviewDetailsProps {
  tools: readonly Tool[];
  previews: readonly ToolUpdatePreview[];
}

function previewToolId(preview: ToolUpdatePreview) {
  return preview.state === "ready" ? preview.preview.tool : preview.tool;
}

function fallbackKey(method: ToolUpdateMethod) {
  return `tools.updatePreview.fallback.${method}` as const;
}

export function UpdatePreviewDetails({
  tools,
  previews,
}: UpdatePreviewDetailsProps) {
  const { t } = useTranslation();
  const toolsById = new Map(tools.map((tool) => [tool.id, tool]));

  return (
    <div className="space-y-3">
      {previews.map((item) => {
        const toolId = previewToolId(item);
        const tool = toolsById.get(toolId);
        if (!tool) return null;

        if (item.state === "blocked") {
          return (
            <section
              key={toolId}
              className="rounded-xl border border-warning/30 bg-warning/5 p-4"
            >
              <div className="flex items-start gap-3">
                <ToolArtwork toolId={toolId} className="h-10 w-10 rounded-lg" />
                <div className="min-w-0 flex-1">
                  <div className="flex flex-wrap items-center gap-2">
                    <h3 className="text-body font-semibold text-content">
                      {t("tools.updatePreview.skipped", { name: tool.name })}
                    </h3>
                    <Badge tone="warning" icon={AlertTriangle}>
                      {t("tools.updatePreview.needsReview")}
                    </Badge>
                  </div>
                  <p className="mt-1 text-caption leading-5 text-content-muted">
                    {t(`tools.updatePreview.blocked.${item.reason}`)}
                  </p>
                </div>
              </div>
            </section>
          );
        }

        const { preview } = item;
        const primary = preview.attempts[0];
        const fallback = preview.attempts[1];
        const selected = preview.installations.find(
          (installation) => installation.isDefault,
        );

        return (
          <section
            key={toolId}
            className="overflow-hidden rounded-xl border border-success/25 bg-success/5"
          >
            <div className="p-4">
              <div className="flex items-start gap-3">
                <ToolArtwork toolId={toolId} className="h-10 w-10 rounded-lg" />
                <div className="min-w-0 flex-1">
                  <div className="flex flex-wrap items-center gap-2">
                    <h3 className="text-body font-semibold text-content">
                      {t("tools.updatePreview.ready", { name: tool.name })}
                    </h3>
                    <Badge tone="success" icon={CheckCircle2}>
                      {t("tools.updatePreview.checked")}
                    </Badge>
                  </div>
                  <p className="mt-1 text-caption font-medium text-content-muted">
                    {t("tools.updatePreview.targetVersion", {
                      version: preview.targetVersion,
                    })}
                  </p>
                  {primary ? (
                    <p className="mt-1 text-body text-content">
                      {t(`tools.updatePreview.method.${primary.method}`, {
                        name: tool.name,
                      })}
                    </p>
                  ) : null}
                  {fallback ? (
                    <p className="mt-1 text-caption leading-5 text-content-muted">
                      {t(fallbackKey(fallback.method), { name: tool.name })}
                    </p>
                  ) : null}
                </div>
              </div>

              {preview.multipleInstallations ? (
                <div className="mt-3 flex items-start gap-2 rounded-lg border border-warning/25 bg-layer-1 px-3 py-2.5">
                  <Layers3
                    className="mt-0.5 h-4 w-4 shrink-0 text-warning"
                    aria-hidden="true"
                  />
                  <div className="min-w-0">
                    <p className="text-caption font-medium text-content">
                      {t("tools.updatePreview.multipleInstallations", {
                        count: preview.installations.length,
                      })}
                    </p>
                    <p className="mt-0.5 text-caption text-content-muted">
                      {t("tools.updatePreview.currentDefault")}
                      {selected?.version
                        ? ` · ${t("tools.updatePreview.version", { version: selected.version })}`
                        : null}
                    </p>
                  </div>
                </div>
              ) : null}
            </div>

            <details className="group border-t border-hairline bg-layer-1">
              <summary className="flex cursor-pointer list-none items-center justify-between gap-3 px-4 py-3 text-caption font-medium text-content-muted transition-colors hover:text-content">
                <span className="flex items-center gap-2">
                  <TerminalSquare className="h-4 w-4" aria-hidden="true" />
                  {t("tools.updatePreview.technicalDetails")}
                </span>
                <ChevronDown
                  className="h-4 w-4 transition-transform duration-fast group-open:rotate-180"
                  aria-hidden="true"
                />
              </summary>
              <div className="space-y-4 border-t border-hairline px-4 py-3">
                <div>
                  <p className="text-caption font-medium text-content">
                    {t("tools.updatePreview.commands")}
                  </p>
                  <ol className="mt-2 space-y-2">
                    {preview.attempts.flatMap((attempt, attemptIndex) =>
                      attempt.commands.map((command, commandIndex) => (
                        <li
                          key={`${attemptIndex}-${commandIndex}`}
                          className="min-w-0 rounded-md bg-layer-1 px-3 py-2"
                        >
                          <code
                            data-selectable-text
                            className="block break-all font-mono text-mono-sm text-content-muted"
                          >
                            {command}
                          </code>
                        </li>
                      )),
                    )}
                  </ol>
                </div>

                <div>
                  <p className="text-caption font-medium text-content">
                    {t("tools.updatePreview.installations")}
                  </p>
                  <ul className="mt-2 space-y-2">
                    {preview.installations.map((installation) => (
                      <li
                        key={installation.location}
                        className="rounded-md border border-hairline bg-layer-1 px-3 py-2"
                      >
                        <div className="flex flex-wrap items-center gap-2">
                          <Badge
                            tone={installation.isDefault ? "brand" : "neutral"}
                          >
                            {installation.isDefault
                              ? t("tools.updatePreview.currentDefault")
                              : t("tools.updatePreview.otherInstallation")}
                          </Badge>
                          <span className="text-caption text-content-muted">
                            {t(
                              `tools.updatePreview.source.${installation.source}`,
                            )}
                          </span>
                          {installation.version ? (
                            <span className="text-caption text-content-muted">
                              {installation.version}
                            </span>
                          ) : null}
                        </div>
                        <div className="mt-2 flex min-w-0 items-start gap-2">
                          <MapPin
                            className="mt-0.5 h-3.5 w-3.5 shrink-0 text-content-subtle"
                            aria-hidden="true"
                          />
                          <code
                            data-selectable-text
                            className="min-w-0 break-all font-mono text-mono-sm text-content-muted"
                          >
                            {installation.location}
                          </code>
                        </div>
                      </li>
                    ))}
                  </ul>
                </div>

                <p className="text-caption leading-5 text-content-subtle">
                  {t("tools.updatePreview.rechecked")}
                </p>
              </div>
            </details>
          </section>
        );
      })}
    </div>
  );
}
