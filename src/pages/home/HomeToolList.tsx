import { Download } from "lucide-react";
import { useId } from "react";
import { useTranslation } from "react-i18next";
import type { Tool, ToolId } from "@/entities/tool";
import { isToolLaunchable } from "@/features/tool-management";
import { Button } from "@/shared/ui/Button";
import { ListGroup } from "@/shared/ui/ListGroup";
import { HomeToolRow } from "./HomeToolRow";

export interface HomeToolListProps {
  tools: readonly Tool[];
  onInstall: () => void;
  onOpenTool: (tool: Tool) => void;
  onOpenServices: (toolId: ToolId) => void;
}

/** Each installed tool on one row, with the endpoint it uses beside it. */
export function HomeToolList({
  tools,
  onInstall,
  onOpenTool,
  onOpenServices,
}: HomeToolListProps) {
  const { t } = useTranslation();
  const headingId = useId();

  return (
    <section
      aria-labelledby={headingId}
      className="flex min-w-0 flex-col gap-2"
    >
      <div className="flex min-w-0 items-center justify-between gap-3">
        <h2 id={headingId} className="text-heading text-content">
          {t("home.tools.title")}
        </h2>
        <Button variant="ghost" size="xs" onClick={onInstall}>
          <Download className="h-3.5 w-3.5" aria-hidden="true" />
          {t("home.tools.install")}
        </Button>
      </div>
      {tools.length > 0 ? (
        <ListGroup>
          {tools.map((tool) => (
            <HomeToolRow
              key={tool.id}
              tool={tool}
              onOpenTool={
                isToolLaunchable(tool) ? () => onOpenTool(tool) : undefined
              }
              onOpenServices={onOpenServices}
            />
          ))}
        </ListGroup>
      ) : (
        <p className="rounded-lg border border-dashed border-hairline px-4 py-3 text-caption text-content-muted">
          {t("home.tools.empty")}
        </p>
      )}
    </section>
  );
}
