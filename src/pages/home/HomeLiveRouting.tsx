import { isLiveRoutingOn, useRoutingOverview } from "@/entities/routing";
import { LiveRoutingPanel, LiveRoutingSwitch } from "@/features/live-routing";
import { Card } from "@/shared/ui/Card";

/**
 * The live routing switch, with the live request list under it while the
 * mode is on (ADR-0050). Off, the home page keeps only the switch, so a user
 * on direct connections never sees an empty request list.
 */
export function HomeLiveRouting() {
  const overview = useRoutingOverview();
  const on = overview.data ? isLiveRoutingOn(overview.data) : false;

  return (
    <Card padding="sm" className="flex flex-col gap-3">
      <LiveRoutingSwitch />
      {on ? <LiveRoutingPanel /> : null}
    </Card>
  );
}
