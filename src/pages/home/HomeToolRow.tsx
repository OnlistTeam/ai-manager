import { useId } from "react";
import type { Tool, ToolId } from "@/entities/tool";
import { ListGroupRow } from "@/shared/ui/ListGroup";
import { ToolGlyph } from "@/shared/ui/ToolArtwork";
import { HomeEffortPicker } from "./HomeEffortPicker";
import { HomeModelPicker } from "./HomeModelPicker";
import { HomeServicePicker } from "./HomeServicePicker";
import { HomeToolRowNotice } from "./HomeToolRowNotice";
import { useHomeToolConnection } from "./useHomeToolConnection";

export interface HomeToolRowProps {
  tool: Tool;
  /** Offered on the switch toast; undefined when the tool cannot launch. */
  onOpenTool?: () => void;
  onOpenServices: (toolId: ToolId) => void;
}

/**
 * One installed tool on one line: which tool, then two pickers at the
 * trailing edge, the model (its endpoint's models grouped by endpoint) and
 * the thinking effort (ADR-0055). A tool whose model is chosen inside the
 * tool keeps the endpoint picker. A line under it appears only when the last switch from this
 * row left something to say.
 */
export function HomeToolRow({
  tool,
  onOpenTool,
  onOpenServices,
}: HomeToolRowProps) {
  const headingId = useId();
  const connection = useHomeToolConnection(tool, onOpenTool);
  const openServices = () => onOpenServices(tool.id);
  const state = connection.connection.kind;
  const ServicePicker =
    tool.capabilities.canChooseModel &&
    state !== "loading" &&
    state !== "unavailable" &&
    state !== "added"
      ? HomeModelPicker
      : HomeServicePicker;

  return (
    <ListGroupRow
      role="article"
      aria-labelledby={headingId}
      aria-busy={
        connection.connection.kind === "loading" ||
        connection.switchingName !== null ||
        connection.settingModel ||
        connection.settingEffort
          ? true
          : undefined
      }
      className="flex flex-col gap-1.5 py-1.5"
    >
      <div className="flex min-w-0 items-center gap-3">
        <span className="flex h-7 w-7 shrink-0 items-center justify-center rounded-md border border-hairline bg-layer-1">
          <ToolGlyph toolId={tool.id} className="h-4 w-4" />
        </span>
        <h3
          id={headingId}
          className="min-w-0 flex-1 truncate text-body font-medium text-content"
        >
          {tool.name}
        </h3>
        <div className="flex shrink-0 items-center gap-2">
          <ServicePicker
            tool={tool}
            connection={connection}
            onOpenServices={openServices}
          />
          <HomeEffortPicker
            tool={tool}
            choice={connection.choice}
            unavailable={connection.choiceUnavailable}
            busy={connection.settingEffort}
            onChoose={connection.chooseEffort}
          />
        </div>
      </div>
      <HomeToolRowNotice
        connection={connection}
        onOpenServices={openServices}
      />
    </ListGroupRow>
  );
}
