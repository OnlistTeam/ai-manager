import { Compass, ExternalLink, UserRoundCheck } from "lucide-react";
import { useState } from "react";
import { useTranslation } from "react-i18next";
import type { Tool, ToolDiscovery, ToolUseCase } from "@/entities/tool";
import { Button } from "@/shared/ui/Button";
import { Card } from "@/shared/ui/Card";
import { ToolArtwork } from "@/shared/ui/ToolArtwork";

const USE_CASES: readonly ToolUseCase[] = [
  "officialCoding",
  "modelChoice",
  "personalAutomation",
];

type DiscoverableTool = Tool & { discovery: ToolDiscovery };

function isDiscoverable(tool: Tool): tool is DiscoverableTool {
  return tool.status === "notInstalled" && tool.discovery !== undefined;
}

export interface ToolDiscoveryGuideProps {
  tools: readonly Tool[];
  onChoose?: (tool: Tool) => void;
}

/**
 * A bounded comparison of the product registry, driven entirely by native
 * discovery metadata. This is intentionally not a searchable marketplace.
 */
export function ToolDiscoveryGuide({
  tools,
  onChoose,
}: ToolDiscoveryGuideProps) {
  const { t } = useTranslation();
  const candidates = tools.filter(isDiscoverable);
  const availableUseCases = USE_CASES.filter((useCase) =>
    candidates.some((tool) => tool.discovery.useCases.includes(useCase)),
  );
  const [requestedUseCase, setRequestedUseCase] =
    useState<ToolUseCase>("officialCoding");
  const activeUseCase = availableUseCases.includes(requestedUseCase)
    ? requestedUseCase
    : availableUseCases[0];

  if (candidates.length === 0 || activeUseCase === undefined) return null;

  const recommendations = candidates.filter((tool) =>
    tool.discovery.useCases.includes(activeUseCase),
  );
  const titleId = "tool-discovery-title";

  return (
    <section aria-labelledby={titleId}>
      <Card padding="lg" className="overflow-hidden bg-layer-1">
        <div className="flex items-start gap-3">
          <span className="flex h-10 w-10 shrink-0 items-center justify-center rounded-xl bg-brand/10 text-brand">
            <Compass className="h-5 w-5" aria-hidden="true" />
          </span>
          <div className="min-w-0 text-left">
            <p className="text-overline text-brand">
              {t("tools.discovery.eyebrow")}
            </p>
            <h2 id={titleId} className="mt-1 text-heading text-content">
              {t("tools.discovery.title")}
            </h2>
            <p className="mt-1 max-w-3xl text-body leading-6 text-content-muted">
              {t("tools.discovery.description")}
            </p>
          </div>
        </div>

        <div
          className="mt-5 flex flex-wrap gap-2"
          role="group"
          aria-label={t("tools.discovery.scenarioLabel")}
        >
          {availableUseCases.map((useCase) => (
            <Button
              key={useCase}
              size="sm"
              variant={activeUseCase === useCase ? "primary" : "secondary"}
              aria-pressed={activeUseCase === useCase}
              onClick={() => setRequestedUseCase(useCase)}
            >
              {t(`tools.discovery.scenario.${useCase}`)}
            </Button>
          ))}
        </div>

        <div className="mt-4 grid gap-3 md:grid-cols-2" role="list">
          {recommendations.map((tool) => (
            <article
              key={tool.id}
              role="listitem"
              className="flex min-w-0 flex-col rounded-xl border border-hairline bg-layer-1 p-4 text-left"
            >
              <div className="flex items-start gap-3">
                <ToolArtwork toolId={tool.id} className="h-10 w-10" />
                <div className="min-w-0">
                  <h3 className="text-body font-semibold text-content">
                    {tool.name}
                  </h3>
                  <p className="mt-0.5 text-caption text-content-muted">
                    {t("tools.discovery.publisher", {
                      publisher: tool.discovery.publisher,
                    })}
                  </p>
                </div>
              </div>
              <p className="mt-3 text-caption leading-5 text-content-muted">
                {t(tool.descriptionKey)}
              </p>
              <p className="mt-3 flex items-start gap-2 rounded-lg bg-layer-1 px-3 py-2 text-caption leading-5 text-content">
                <UserRoundCheck
                  className="mt-0.5 h-4 w-4 shrink-0 text-brand"
                  aria-hidden="true"
                />
                <span>
                  {t(`tools.discovery.access.${tool.discovery.access}`)}
                </span>
              </p>
              {onChoose ? (
                <Button
                  size="sm"
                  variant="ghost"
                  className="mt-3 self-start"
                  onClick={() => onChoose(tool)}
                >
                  <ExternalLink className="h-4 w-4" aria-hidden="true" />
                  {t("tools.discovery.choose", { name: tool.name })}
                </Button>
              ) : null}
            </article>
          ))}
        </div>

        <p className="mt-4 text-caption leading-5 text-content-muted">
          {t("tools.discovery.costNote")}
        </p>
      </Card>
    </section>
  );
}
