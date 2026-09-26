import type { RefObject } from "react";
import { useTranslation } from "react-i18next";
import type { ToolId } from "@/entities/routing";
import { cn } from "@/shared/ui/cn";
import {
  StageAllToolsNode,
  StageEndpointRow,
  StageHub,
  StagePlaceholder,
  StageToolNode,
} from "./LiveRoutingStageNodes";
import type { EndpointMark, StageEndpoint } from "./stageModel";
import { toolHue } from "./stageModel";
import type { StageWiresState } from "./useStageWires";

export interface LiveRoutingStageProps {
  stageRef: RefObject<HTMLDivElement>;
  wires: StageWiresState;
  compact: boolean;
  /** Tools with their own node; empty shows the one "your tools" node. */
  tools: readonly ToolId[];
  /** Tools the "your tools" node stands for. */
  routedTools: readonly ToolId[];
  address: string | null;
  /** Null before the first request: the dashed placeholder is shown. */
  endpoints: readonly StageEndpoint[] | null;
  marks: ReadonlyMap<string, EndpointMark>;
  hideEmails: boolean;
  /** Wires a request is waiting on, with the tool that sent it. */
  held: ReadonlyMap<string, ToolId>;
  /** Requests with a dot in the air or waiting on a wire. */
  dots: readonly number[];
}

const COLUMNS = {
  wide: "grid-cols-[minmax(0,9.5rem)_minmax(1.5rem,1fr)_auto_minmax(1.5rem,1fr)_minmax(0,15rem)]",
  compact:
    "grid-cols-[auto_minmax(1rem,1fr)_auto_minmax(1rem,1fr)_minmax(0,11rem)]",
};

/**
 * Tools on the left, AI Manager in the middle, services on the right, and
 * wires between them drawn underneath. The dots are moved by the flight
 * player; this component only renders them hidden in place.
 */
export function LiveRoutingStage({
  stageRef,
  wires,
  compact,
  tools,
  routedTools,
  address,
  endpoints,
  marks,
  hideEmails,
  held,
  dots,
}: LiveRoutingStageProps) {
  const { t } = useTranslation();

  return (
    <div
      ref={stageRef}
      role="group"
      aria-label={t("routing.live.stage.label")}
      data-live-stage=""
      className="relative px-3 py-4"
    >
      <svg
        aria-hidden="true"
        width={wires.width}
        height={wires.height}
        className="pointer-events-none absolute left-0 top-0 overflow-visible"
      >
        {wires.wires.map((wire) => {
          const tool = held.get(wire.id);
          return (
            <path
              key={wire.id}
              data-wire={wire.id}
              data-held={tool ? "" : undefined}
              d={wire.d}
              fill="none"
              strokeWidth={tool ? 2 : 1.5}
              strokeLinecap="round"
              strokeDasharray={wire.id === "placeholder" ? "4 4" : undefined}
              className={cn(
                "stroke-hairline-strong",
                tool && "opacity-80 motion-safe:animate-pulse",
              )}
              style={tool ? { stroke: `hsl(${toolHue(tool)})` } : undefined}
            />
          );
        })}
        {dots.map((seq) => (
          <circle
            key={seq}
            data-flight-dot={seq}
            r={4.5}
            visibility="hidden"
            className="stroke-canvas"
            strokeWidth={1.5}
          />
        ))}
      </svg>

      <div
        className={cn(
          "relative grid items-center",
          compact ? COLUMNS.compact : COLUMNS.wide,
        )}
      >
        <div className="col-start-1 flex min-w-0 flex-col gap-2">
          {tools.length > 0 ? (
            tools.map((tool) => (
              <StageToolNode key={tool} tool={tool} compact={compact} />
            ))
          ) : (
            <StageAllToolsNode tools={routedTools} compact={compact} />
          )}
        </div>
        <div className="col-start-3">
          <StageHub address={address} compact={compact} />
        </div>
        <div className="col-start-5 min-w-0">
          {endpoints && endpoints.length > 0 ? (
            <ol
              aria-label={t("routing.live.stage.servicesLabel")}
              className="flex flex-col gap-1.5"
            >
              {endpoints.map((endpoint) => (
                <StageEndpointRow
                  key={endpoint.id}
                  endpoint={endpoint}
                  mark={marks.get(endpoint.id)}
                  hideEmails={hideEmails}
                  compact={compact}
                />
              ))}
            </ol>
          ) : (
            <StagePlaceholder
              label={t(
                endpoints
                  ? "routing.live.noAttempt"
                  : "routing.live.stage.noRequests",
              )}
            />
          )}
        </div>
      </div>
    </div>
  );
}
