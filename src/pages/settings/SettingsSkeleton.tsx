import { DetectionStatus } from "@/shared/ui/DetectionStatus";

export interface SettingsSkeletonProps {
  label: string;
}

export function SettingsSkeleton({ label }: SettingsSkeletonProps) {
  return <DetectionStatus label={label} />;
}
