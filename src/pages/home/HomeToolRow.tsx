import { useId } from "react";
import type { Tool, ToolId } from "@/entities/tool";
import { ListGroupRow } from "@/shared/ui/ListGroup";
import { ToolGlyph } from "@/shared/ui/ToolArtwork";
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
 * One installed tool on one line: which tool, and the endpoint it uses as a
 * picker at the trailing edge. A line under it appears only when the last
 * switch from this row left something to say.
 */
export function HomeToolRow({
  tool,
  onOpenTool,
  onOpenServices,
}: HomeToolRowProps) {
  const headingId = useId();
  const connection = useHomeToolConnection(tool, onOpenTool);
  const openServices = () => onOpenServices(tool.id);

  return (
    <ListGroupRow
      role="article"
      aria-labelledby={headingId}
      aria-busy={
        connection.connection.kind === "loading" ||
        connection.switchingName !== null
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
        <HomeServicePicker
          tool={tool}
          connection={connection}
          onOpenServices={openServices}
        />
      </div>
      <HomeToolRowNotice
        connection={connection}
        onOpenServices={openServices}
      />
    </ListGroupRow>
  );
}
