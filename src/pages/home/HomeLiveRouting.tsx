import { isLiveRoutingOn, useRoutingOverview } from "@/entities/routing";
import { LiveRoutingPanel, LiveRoutingSwitch } from "@/features/live-routing";
import { PrivacyStatusLine } from "@/features/routing-privacy";
import { Card } from "@/shared/ui/Card";

export interface HomeLiveRoutingProps {
  /** Opens the privacy section of the settings page. */
  onOpenPrivacySettings?: () => void;
}

/**
 * The live routing switch, with what privacy protection hides (ADR-0049)
 * and the live request list under it while the mode is on (ADR-0050). Off,
 * the home page keeps only the switch: both only mean something for
 * requests that go through AI Manager.
 */
export function HomeLiveRouting({
  onOpenPrivacySettings,
}: HomeLiveRoutingProps) {
  const overview = useRoutingOverview();
  const on = overview.data ? isLiveRoutingOn(overview.data) : false;

  return (
    <Card padding="sm" className="flex flex-col gap-3">
      <LiveRoutingSwitch />
      {on ? (
        <>
          <PrivacyStatusLine
            onOpenSettings={onOpenPrivacySettings}
            className="border-t border-hairline pt-3"
          />
          <LiveRoutingPanel />
        </>
      ) : null}
    </Card>
  );
}
