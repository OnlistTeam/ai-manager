import { Download, RefreshCw } from "lucide-react";
import { useId } from "react";
import { useTranslation } from "react-i18next";
import type { Tool, ToolId } from "@/entities/tool";
import { isToolLaunchable } from "@/features/tool-management";
import { Button } from "@/shared/ui/Button";
import { ListGroup } from "@/shared/ui/ListGroup";
import { HomeToolRow, type HomeToolRowUpdate } from "./HomeToolRow";

export interface HomeUpdateAllAction {
  disabled: boolean;
  loading: boolean;
  /** Why the button is disabled; spelled out rather than only greyed out. */
  hintKey?: string;
  onSelect: () => void;
}

export interface HomeToolListProps {
  tools: readonly Tool[];
  /** Present only while at least one installed tool has an update. */
  updateAll: HomeUpdateAllAction | null;
  updateFor: (tool: Tool) => HomeToolRowUpdate | null;
  onInstall: () => void;
  onOpenTool: (tool: Tool) => void;
  onOpenServices: (toolId: ToolId) => void;
}

/** Each installed tool on one row, answering what it is connected to now. */
export function HomeToolList({
  tools,
  updateAll,
  updateFor,
  onInstall,
  onOpenTool,
  onOpenServices,
}: HomeToolListProps) {
  const { t } = useTranslation();
  const headingId = useId();
  const hintId = useId();

  return (
    <section
      aria-labelledby={headingId}
      className="flex min-w-0 flex-col gap-2"
    >
      <div className="flex min-w-0 flex-wrap items-center gap-x-3 gap-y-1">
        <h2 id={headingId} className="text-heading text-content">
          {t("home.tools.title")}
        </h2>
        <div className="ml-auto flex flex-wrap items-center justify-end gap-2">
          {updateAll?.hintKey ? (
            <span id={hintId} className="text-caption text-content-muted">
              {t(updateAll.hintKey)}
            </span>
          ) : null}
          {updateAll ? (
            <Button
              variant="secondary"
              size="xs"
              disabled={updateAll.disabled}
              loading={updateAll.loading}
              aria-describedby={updateAll.hintKey ? hintId : undefined}
              onClick={updateAll.onSelect}
            >
              {updateAll.loading ? null : (
                <RefreshCw className="h-3.5 w-3.5" aria-hidden="true" />
              )}
              {t("home.tools.updateAll")}
            </Button>
          ) : null}
          <Button variant="ghost" size="xs" onClick={onInstall}>
            <Download className="h-3.5 w-3.5" aria-hidden="true" />
            {t("home.tools.install")}
          </Button>
        </div>
      </div>
      {tools.length > 0 ? (
        <ListGroup>
          {tools.map((tool) => (
            <HomeToolRow
              key={tool.id}
              tool={tool}
              update={updateFor(tool)}
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
