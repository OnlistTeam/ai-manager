import { Check, Ellipsis, SkipForward, X } from "lucide-react";
import { useTranslation } from "react-i18next";
import brandMark from "@/assets/spatial/models/v5/mark.png";
import type { ToolId } from "@/entities/routing";
import { cn } from "@/shared/ui/cn";
import { ToolGlyph } from "@/shared/ui/ToolArtwork";
import { endpointWire, toolWire } from "./flightPlan";
import { formatDuration, serviceLabel } from "./liveRoutingFormat";
import { toolHue, type EndpointMark, type StageEndpoint } from "./stageModel";

const NODE =
  "relative flex min-w-0 items-center gap-2 rounded-lg border border-hairline bg-layer-1 text-caption";

/** A tool on the left; it glows briefly (see flightDom) when it sends. */
export function StageToolNode({
  tool,
  compact,
}: {
  tool: ToolId;
  compact: boolean;
}) {
  const { t } = useTranslation();
  const name = t(`routing.tool.${tool}`);
  return (
    <div
      data-stage-source={toolWire(tool)}
      data-stage-tool={tool}
      title={name}
      className={cn(NODE, "h-8", compact ? "w-8 justify-center" : "px-2")}
    >
      <ToolGlyph toolId={tool} className="h-4 w-4 shrink-0" />
      <span className={compact ? "sr-only" : "truncate text-content"}>
        {name}
      </span>
      <span
        aria-hidden="true"
        className="absolute -right-[3px] top-1/2 h-1.5 w-1.5 -translate-y-1/2 rounded-full"
        style={{ backgroundColor: `hsl(${toolHue(tool)})` }}
      />
    </div>
  );
}

/** Before any request: one node standing for every routed tool. */
export function StageAllToolsNode({
  tools,
  compact,
}: {
  tools: readonly ToolId[];
  compact: boolean;
}) {
  const { t } = useTranslation();
  return (
    <div data-stage-source="tool:all" className={cn(NODE, "h-8 px-2")}>
      <span className="flex shrink-0 -space-x-1">
        {tools.map((tool) => (
          <ToolGlyph key={tool} toolId={tool} className="h-4 w-4" />
        ))}
      </span>
      <span className={compact ? "sr-only" : "truncate text-content-muted"}>
        {t("routing.live.stage.yourTools")}
      </span>
    </div>
  );
}

/** AI Manager in the middle, with the local address it listens on. */
export function StageHub({
  address,
  compact,
}: {
  address: string | null;
  compact: boolean;
}) {
  const { t } = useTranslation();
  return (
    <div
      data-stage-hub=""
      className={cn(
        NODE,
        "flex-col justify-center gap-0.5 border-brand/30 px-3 py-2 text-center",
      )}
    >
      <img
        src={brandMark}
        alt=""
        draggable={false}
        className="h-7 w-7 shrink-0"
      />
      <span className="whitespace-nowrap font-medium text-content">
        {t("routing.live.stage.hub")}
      </span>
      {address && !compact ? (
        <span className="whitespace-nowrap font-mono text-content-muted">
          {address}
        </span>
      ) : null}
    </div>
  );
}

function Mark({
  mark,
  compact,
}: {
  mark: EndpointMark | undefined;
  compact: boolean;
}) {
  const { t, i18n } = useTranslation();
  if (!mark) return null;
  const [Icon, tone, text] =
    mark.kind === "ok"
      ? [Check, "text-success", formatDuration(mark.ms, i18n.language)]
      : mark.kind === "failed"
        ? [X, "text-danger", t(`routing.live.errorCategory.${mark.error}`)]
        : mark.kind === "pending"
          ? [
              Ellipsis,
              "text-content-muted motion-safe:animate-pulse",
              t("routing.live.stage.waiting"),
            ]
          : [
              SkipForward,
              "text-content-muted",
              t("routing.live.stage.resting"),
            ];
  return (
    <span
      data-endpoint-mark={mark.kind}
      title={text}
      className="flex shrink-0 items-center gap-1 text-content-muted"
    >
      <Icon className={cn("h-3.5 w-3.5 shrink-0", tone)} aria-hidden="true" />
      <span className={compact ? "sr-only" : "whitespace-nowrap tabular-nums"}>
        {text}
      </span>
    </span>
  );
}

/** One service on the right, in routing order, with how its try went. */
export function StageEndpointRow({
  endpoint,
  mark,
  hideEmails,
  compact,
}: {
  endpoint: StageEndpoint;
  mark: EndpointMark | undefined;
  hideEmails: boolean;
  compact: boolean;
}) {
  const name = serviceLabel(endpoint.name, hideEmails);
  return (
    <li
      data-stage-target={endpointWire(endpoint.id)}
      data-endpoint={endpoint.id}
      className={cn(
        NODE,
        "h-8 justify-between px-2.5",
        mark?.kind === "pending" && "border-brand/40",
        mark?.kind === "failed" && "border-danger/40",
      )}
    >
      <span title={name} className="min-w-0 truncate text-content">
        {name}
      </span>
      <Mark mark={mark} compact={compact} />
    </li>
  );
}

/** Where the services will appear once a request arrives. */
export function StagePlaceholder({ label }: { label: string }) {
  return (
    <div
      data-stage-target="placeholder"
      className="flex h-16 items-center justify-center rounded-lg border border-dashed border-hairline-strong px-3 text-center text-caption text-content-muted"
    >
      {label}
    </div>
  );
}
