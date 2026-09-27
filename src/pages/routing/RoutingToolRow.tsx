import { AlertTriangle, Route } from "lucide-react";
import { useId, type ReactNode } from "react";
import { useTranslation } from "react-i18next";
import type { RoutingTarget } from "@/entities/routing";
import { ListGroupRow } from "@/shared/ui/ListGroup";
import { Switch } from "@/shared/ui/Switch";
import { ToolGlyph } from "@/shared/ui/ToolArtwork";

export interface RoutingToolRowProps {
  target: RoutingTarget;
  busy: boolean;
  /** What an open session of the tool does after the last change here. */
  note: string | null;
  onRoutedChange: (routed: boolean) => void;
  /** The failover controls under a routed row. */
  children?: ReactNode;
}

/**
 * One tool on one line: the endpoint it uses and whether it goes through AI
 * Manager (ADR-0054). A tool whose connection cannot be forwarded shows why
 * instead of a switch.
 */
export function RoutingToolRow({
  target,
  busy,
  note,
  onRoutedChange,
  children,
}: RoutingToolRowProps) {
  const { t } = useTranslation();
  const headingId = useId();
  const noteId = useId();
  const name = t(`routing.tool.${target.tool}`);
  const routed = target.takeoverEnabled;
  const blocked = target.unavailable;
  const endpoint = target.currentProvider?.name ?? t("routing.row.noEndpoint");

  return (
    <ListGroupRow
      role="article"
      aria-labelledby={headingId}
      data-routing-target={target.tool}
      className="flex flex-col gap-1.5 py-2"
    >
      <div className="flex min-w-0 items-center gap-3">
        <span className="flex h-7 w-7 shrink-0 items-center justify-center rounded-md border border-hairline bg-layer-1">
          <ToolGlyph toolId={target.tool} className="h-4 w-4" />
        </span>
        <div className="min-w-0 flex-1">
          <h3
            id={headingId}
            className="truncate text-body font-medium text-content"
          >
            {name}
          </h3>
          <p className="flex min-w-0 items-center gap-1.5 text-caption text-content-muted">
            <span className="truncate">{endpoint}</span>
            {routed ? (
              <span className="flex shrink-0 items-center gap-1 text-brand">
                <Route className="h-3 w-3" aria-hidden="true" />
                {t("routing.row.through")}
              </span>
            ) : null}
          </p>
        </div>
        {blocked && !routed ? (
          <p
            data-routing-unavailable={blocked}
            className="max-w-[55%] shrink-0 text-right text-caption text-content-muted"
          >
            {t(`routing.unavailable.${blocked}`)}
          </p>
        ) : (
          <Switch
            checked={routed}
            disabled={busy}
            aria-label={t("routing.row.switchLabel", { name })}
            aria-describedby={note ? noteId : undefined}
            onCheckedChange={onRoutedChange}
          />
        )}
      </div>
      {routed && blocked ? (
        <p
          role="alert"
          className="flex items-start gap-1.5 pl-10 text-caption text-content"
        >
          <AlertTriangle
            className="mt-0.5 h-3.5 w-3.5 shrink-0 text-warning"
            aria-hidden="true"
          />
          {t("routing.row.routedButUnavailable")}
        </p>
      ) : null}
      {note ? (
        <p
          id={noteId}
          role="status"
          className="pl-10 text-caption text-content-muted"
        >
          {note}
        </p>
      ) : null}
      {children}
    </ListGroupRow>
  );
}
