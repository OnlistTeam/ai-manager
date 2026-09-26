import { useMemo, useRef } from "react";
import { useTranslation } from "react-i18next";
import type {
  RoutingOverview,
  RoutingTraceSnapshot,
  ToolId,
} from "@/entities/routing";
import { heldWire, planFlight, revealedAttempts } from "./flightPlan";
import { liveCaption } from "./liveCaption";
import { LiveRoutingCounters } from "./LiveRoutingCounters";
import { LiveRoutingList } from "./LiveRoutingList";
import { LiveRoutingStage } from "./LiveRoutingStage";
import { endpointMarks, stageEndpoints, stageTools } from "./stageModel";
import { useFlightPlayer } from "./useFlightPlayer";
import { usePrefersReducedMotion } from "./usePrefersReducedMotion";
import { useStageWires } from "./useStageWires";

/** Below this stage width tool nodes show only their icon. */
const COMPACT_BELOW = 560;

/**
 * The live stage, the sentence under it and the recent requests. The latest request decides
 * which tool's services are on the right and what the caption says; every
 * request still in flight has its own dot.
 */
export function LiveRoutingLive({
  overview,
  snapshot,
  hideEmails,
}: {
  overview: RoutingOverview | undefined;
  snapshot: RoutingTraceSnapshot;
  hideEmails: boolean;
}) {
  const { t, i18n } = useTranslation();
  const reducedMotion = usePrefersReducedMotion();
  const stageRef = useRef<HTMLDivElement>(null);
  const { entries } = snapshot;
  const latest = entries[0];
  const target = overview?.targets.find((item) => item.tool === latest?.tool);
  const endpoints = useMemo(
    () => (latest ? stageEndpoints(target, latest) : null),
    [target, latest],
  );
  const tools = useMemo(
    () => (entries.length > 0 ? stageTools(overview, entries) : []),
    [overview, entries],
  );
  const routedTools = stageTools(overview, []);

  const wires = useStageWires(stageRef);
  const compact = wires.width > 0 && wires.width < COMPACT_BELOW;
  const tracks = useFlightPlayer(entries, stageRef, reducedMotion, wires);

  const latestPlan = latest ? planFlight(latest) : null;
  const latestDone =
    latest && latestPlan
      ? (tracks.get(latest.seq)?.done ?? latestPlan.legs.length)
      : 0;
  const marks = endpointMarks(
    endpoints ?? [],
    latest,
    latestPlan ? revealedAttempts(latestPlan, latestDone) : 0,
  );
  const held = new Map<string, ToolId>();
  for (const track of tracks.values()) {
    const wire = heldWire(track.plan, track.done);
    if (wire) held.set(wire, track.tool);
  }
  const caption =
    latest && latestPlan
      ? liveCaption(latest, latestPlan, latestDone, {
          t,
          language: i18n.language,
          hideEmails,
        })
      : t("routing.live.empty");

  return (
    <>
      <LiveRoutingStage
        stageRef={stageRef}
        wires={wires}
        compact={compact}
        tools={tools}
        routedTools={routedTools}
        address={
          overview?.address && overview.port
            ? `${overview.address}:${overview.port}`
            : null
        }
        endpoints={endpoints}
        marks={marks}
        hideEmails={hideEmails}
        held={held}
        dots={reducedMotion ? [] : [...tracks.keys()]}
      />
      <div className="flex flex-wrap items-center justify-between gap-x-4 gap-y-1 border-t border-hairline px-3 py-2">
        <p
          aria-live="polite"
          data-live-caption=""
          className="min-w-0 flex-1 text-caption text-content"
        >
          {caption}
        </p>
        <LiveRoutingCounters counts={snapshot.counts} />
      </div>
      {entries.length > 0 ? (
        <LiveRoutingList
          entries={entries}
          hideEmails={hideEmails}
          compact={compact}
        />
      ) : null}
    </>
  );
}
