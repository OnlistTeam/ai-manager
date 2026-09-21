import type { ComponentType } from "react";
import {
  ArrowUpCircle,
  CheckCircle2,
  CloudOff,
  DownloadCloud,
  Loader2,
  RotateCw,
  Wrench,
} from "lucide-react";
import type { AppUpdatePhase } from "@/entities/update";
import type { BadgeTone } from "@/shared/ui/Badge";

export type UpdateTone =
  | "checking"
  | "downloading"
  | "ready"
  | "available"
  | "upToDate"
  | "unknown"
  | "unconfigured";

export interface UpdateToneInput {
  checking: boolean;
  failed: boolean;
  phase?: AppUpdatePhase;
  channelReady: boolean | undefined;
  availableVersion: string | null | undefined;
}

/**
 * The order of state checks is deliberate: **a failed check must never
 * display as "up to date"**. Being offline is a normal state, and "up to
 * date" would be a lie while offline.
 */
export function updateTone({
  checking,
  failed,
  phase,
  channelReady,
  availableVersion,
}: UpdateToneInput): UpdateTone {
  if (checking) return "checking";
  if (failed || phase === "failed") return "unknown";
  if (phase === "downloading") return "downloading";
  if (phase === "ready") return "ready";
  if (phase === "checking" || phase === "idle") return "checking";
  if (phase === "upToDate") return "upToDate";
  if (phase === "unconfigured") return "unconfigured";
  if (channelReady === false) return "unconfigured";
  return availableVersion ? "available" : "upToDate";
}

/** §98: icon + text + color triple-encoded — state must never be conveyed by color alone. */
export const UPDATE_BADGES: Record<
  UpdateTone,
  {
    tone: BadgeTone;
    icon: ComponentType<{ className?: string }>;
    labelKey: string;
  }
> = {
  checking: {
    tone: "neutral",
    icon: Loader2,
    labelKey: "preferences.updates.checking",
  },
  downloading: {
    tone: "brand",
    icon: DownloadCloud,
    labelKey: "preferences.updates.downloading",
  },
  ready: {
    tone: "success",
    icon: RotateCw,
    labelKey: "preferences.updates.ready",
  },
  available: {
    tone: "brand",
    icon: ArrowUpCircle,
    labelKey: "preferences.updates.available",
  },
  upToDate: {
    tone: "success",
    icon: CheckCircle2,
    labelKey: "preferences.updates.upToDate",
  },
  unknown: {
    tone: "warning",
    icon: CloudOff,
    labelKey: "preferences.updates.unknown",
  },
  // A build without a signed channel is a development build, not a fault.
  unconfigured: {
    tone: "neutral",
    icon: Wrench,
    labelKey: "preferences.updates.devBuild",
  },
};
