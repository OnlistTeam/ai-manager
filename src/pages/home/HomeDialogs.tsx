import {
  OpenToolModal,
  type useToolLaunchFlow,
} from "@/features/tool-management";
import {
  HomeActionCheckingNotice,
  HomeActionPausedNotice,
} from "./HomeReadinessNotice";

export interface HomeDialogsProps {
  launch: ReturnType<typeof useToolLaunchFlow>;
  actionsBlocked: boolean;
  retrying: boolean;
  refreshFailed: boolean;
}

/**
 * The one confirmation Home can open: Start. It pauses while Home's read
 * authorities are refreshing or failed, and says why.
 */
export function HomeDialogs({
  launch,
  actionsBlocked,
  retrying,
  refreshFailed,
}: HomeDialogsProps) {
  const readiness =
    retrying && !refreshFailed ? (
      <HomeActionCheckingNotice />
    ) : refreshFailed ? (
      <HomeActionPausedNotice />
    ) : null;

  return (
    <OpenToolModal
      tool={launch.tool}
      busy={launch.busy}
      confirmDisabled={actionsBlocked}
      error={launch.error}
      providerRecovery={launch.providerRecovery}
      notice={readiness}
      onOpenChange={launch.onOpenChange}
      onLaunchDefault={() => {
        if (!actionsBlocked) launch.confirm("default");
      }}
      onChooseFolder={() => {
        if (!actionsBlocked) launch.confirm("choose");
      }}
    />
  );
}
