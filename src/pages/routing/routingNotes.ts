import type { RoutingPickup } from "@/entities/routing";

/**
 * The note key for what a session of the tool that is already open does
 * after its routing switch changed (ADR-0054). A tool that rereads its
 * settings follows at once; one that reads them at start keeps its old
 * connection until restarted.
 */
export function restartNoteKey(pickup: RoutingPickup, routed: boolean) {
  const direction = routed ? "on" : "off";
  const when = pickup === "live" ? "Live" : "AtStart";
  return `routing.note.${direction}${when}` as const;
}
