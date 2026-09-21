import { DetectionStatus } from "@/shared/ui/DetectionStatus";

export interface ExtensionsSkeletonProps {
  label: string;
}

export function ExtensionsSkeleton({ label }: ExtensionsSkeletonProps) {
  return <DetectionStatus label={label} />;
}
