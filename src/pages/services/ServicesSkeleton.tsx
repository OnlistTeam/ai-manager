import { DetectionStatus } from "@/shared/ui/DetectionStatus";

export interface ServicesSkeletonProps {
  label: string;
}

export function ServicesSkeleton({ label }: ServicesSkeletonProps) {
  return <DetectionStatus label={label} />;
}
