import { useTranslation } from "react-i18next";
import { restartNoteKey, type RoutingTarget } from "@/entities/routing";
import { ListGroup } from "@/shared/ui/ListGroup";
import { RoutingFailover } from "./RoutingFailover";
import { RoutingToolRow } from "./RoutingToolRow";
import type { RoutingActions } from "./useRoutingActions";

export interface RoutingToolListProps {
  targets: RoutingTarget[];
  actions: RoutingActions;
  disabled: boolean;
}

/** One compact row per tool the gateway can carry. */
export function RoutingToolList({
  targets,
  actions,
  disabled,
}: RoutingToolListProps) {
  const { t } = useTranslation();
  const change = actions.lastChange;

  return (
    <ListGroup>
      {targets.map((target) => {
        const name = t(`routing.tool.${target.tool}`);
        const note =
          change?.tool === target.tool &&
          change.routed === target.takeoverEnabled
            ? t(restartNoteKey(target.pickup, change.routed), { name })
            : null;
        return (
          <RoutingToolRow
            key={target.tool}
            target={target}
            busy={disabled}
            note={note}
            onRoutedChange={(routed) => actions.setRouted(target.tool, routed)}
          >
            {target.takeoverEnabled ? (
              <RoutingFailover
                target={target}
                busy={disabled}
                onFailoverChange={(enabled) =>
                  actions.setFailover(target.tool, enabled)
                }
                onAdd={(id) => actions.addToQueue(target.tool, id)}
                onRemove={(id) => actions.removeFromQueue(target.tool, id)}
                onSwitch={(id) => actions.switchTo(target.tool, id)}
              />
            ) : null}
          </RoutingToolRow>
        );
      })}
    </ListGroup>
  );
}
