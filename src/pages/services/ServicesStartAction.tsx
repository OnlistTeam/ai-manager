import { SquareTerminal } from "lucide-react";
import { useTranslation } from "react-i18next";
import type { Tool } from "@/entities/tool";
import {
  isToolLaunchable,
  OpenToolModal,
  type useToolLaunchFlow,
} from "@/features/tool-management";
import { ServiceActionsPausedNotice } from "@/features/provider-management";
import { Button } from "@/shared/ui/Button";

export type ToolLaunchFlow = ReturnType<typeof useToolLaunchFlow>;

export interface ServicesStartActionProps {
  tool: Tool;
  /**
   * Owned by the caller so other entry points on the same page, such as the
   * switch toast's "Open now", open this very modal.
   */
  launch: ToolLaunchFlow;
  actionsBlocked?: boolean;
}

/**
 * A saved provider is enough to offer the next real step, including for tools
 * whose provider format has no single active record. Installation status and
 * the backend capability remain authoritative for whether launch is offered.
 */
export function ServicesStartAction({
  tool,
  launch,
  actionsBlocked = false,
}: ServicesStartActionProps) {
  const { t } = useTranslation();

  if (!isToolLaunchable(tool)) return null;

  return (
    <>
      <Button
        variant="secondary"
        disabled={actionsBlocked}
        loading={launch.busy}
        onClick={() => {
          if (!actionsBlocked) launch.openTool(tool);
        }}
      >
        {launch.busy ? null : (
          <SquareTerminal className="h-4 w-4" aria-hidden="true" />
        )}
        {t("services.start.action", { tool: tool.name })}
      </Button>

      <OpenToolModal
        tool={launch.tool}
        busy={launch.busy}
        confirmDisabled={actionsBlocked}
        error={launch.error}
        providerRecovery={launch.providerRecovery}
        notice={
          actionsBlocked && !launch.busy ? (
            <ServiceActionsPausedNotice className="mb-0 mt-3" />
          ) : undefined
        }
        onOpenChange={launch.onOpenChange}
        onLaunchDefault={() => {
          if (!actionsBlocked) launch.confirm("default");
        }}
        onChooseFolder={() => {
          if (!actionsBlocked) launch.confirm("choose");
        }}
      />
    </>
  );
}
