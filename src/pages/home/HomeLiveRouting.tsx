import { isLiveRoutingOn, useRoutingOverview } from "@/entities/routing";
import { LiveRoutingPanel, LiveRoutingSwitch } from "@/features/live-routing";
import { PrivacyProtectionSwitch } from "@/features/routing-privacy";
import { Card } from "@/shared/ui/Card";

/**
 * The live routing switch, with privacy protection (ADR-0049) and the live
 * request list under it while the mode is on (ADR-0050). Off, the home page
 * keeps only the switch: both only mean something for requests that go
 * through AI Manager.
 */
export function HomeLiveRouting() {
  const overview = useRoutingOverview();
  const on = overview.data ? isLiveRoutingOn(overview.data) : false;

  return (
    <Card padding="sm" className="flex flex-col gap-3">
      <LiveRoutingSwitch />
      {on ? (
        <>
          <PrivacyProtectionSwitch className="border-t border-hairline pb-0" />
          <LiveRoutingPanel />
        </>
      ) : null}
    </Card>
  );
}
