import { DetectionStatus } from "@/shared/ui/DetectionStatus";

export interface ToolsSkeletonProps {
  label: string;
}

/** Kept as a compatibility boundary for the page's first-read state. */
export function ToolsSkeleton({ label }: ToolsSkeletonProps) {
  return <DetectionStatus label={label} />;
}
