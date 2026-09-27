import type { TFunction } from "i18next";
import type { RoutingPickup } from "@/native";

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

export interface RoutingEndedCopy {
  /** The tool's display name. */
  name: string;
  /** The endpoint the tool now uses. */
  endpoint: string;
  pickup: RoutingPickup;
}

/**
 * What to say when changing a tool's endpoint ended its route because the
 * new endpoint cannot go through AI Manager: that routing is off and why,
 * then the note the switch-off path gives for the tool.
 */
export function routingEndedMessage(
  t: TFunction,
  { name, endpoint, pickup }: RoutingEndedCopy,
): string {
  return `${t("routing.ended", { name, endpoint })} ${t(
    restartNoteKey(pickup, false),
    { name },
  )}`;
}
