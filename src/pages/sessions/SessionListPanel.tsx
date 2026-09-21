import { Clock3, Folder, PlayCircle } from "lucide-react";
import { useEffect, useState } from "react";
import { useTranslation } from "react-i18next";
import type { SessionList } from "@/entities/session";
import { Badge } from "@/shared/ui/Badge";
import { Button } from "@/shared/ui/Button";
import { Card } from "@/shared/ui/Card";
import { ToolArtwork } from "@/shared/ui/ToolArtwork";
import { formatSessionTime, sessionActivity } from "./sessionFormat";

const INITIAL_VISIBLE_ITEMS = 60;
const LOAD_MORE_ITEMS = 60;

interface SessionListPanelProps {
  list: SessionList;
  selected: string | null;
  onSelect: (reference: string) => void;
}

export function SessionListPanel({
  list,
  selected,
  onSelect,
}: SessionListPanelProps) {
  const { t, i18n } = useTranslation();
  const locale = i18n.resolvedLanguage ?? i18n.language;
  const [visibleCount, setVisibleCount] = useState(INITIAL_VISIBLE_ITEMS);
  const visibleItems = list.items.slice(0, visibleCount);
  const remaining = list.items.length - visibleItems.length;
  const nextCount = Math.min(LOAD_MORE_ITEMS, remaining);

  useEffect(() => {
    setVisibleCount(INITIAL_VISIBLE_ITEMS);
  }, [list.items]);

  return (
    <Card padding="none" className="min-w-0 overflow-hidden rounded-xl">
      <div className="flex items-center justify-between gap-3 border-b border-hairline px-4 py-3">
        <h2 className="text-heading text-content">
          {t("sessions.list.title")}
        </h2>
        <Badge tone="neutral">
          {t("sessions.list.count", { count: list.totalCount })}
        </Badge>
      </div>
      <ul
        className="max-h-[680px] overflow-y-auto p-2"
        aria-label={t("sessions.list.title")}
      >
        {visibleItems.map((session) => {
          const active = selected === session.reference;
          const time = formatSessionTime(sessionActivity(session), locale);
          return (
            <li key={session.reference}>
              <button
                type="button"
                aria-pressed={active}
                onClick={() => onSelect(session.reference)}
                className={`my-0.5 flex w-full min-w-0 items-start gap-3 rounded-lg border px-3 py-3 text-left transition-colors ${
                  active
                    ? "border-brand/30 bg-brand/10"
                    : "border-transparent hover:border-hairline hover:bg-layer-2"
                }`}
              >
                <ToolArtwork
                  toolId={session.tool}
                  className="h-10 w-10 rounded-lg"
                />
                <span className="min-w-0 flex-1">
                  <span className="flex min-w-0 items-center gap-2">
                    <span className="truncate text-body font-semibold text-content">
                      {session.title ?? t("sessions.list.untitled")}
                    </span>
                    {session.resumable ? (
                      <PlayCircle
                        className="h-3.5 w-3.5 shrink-0 text-success"
                        aria-label={t("sessions.list.resumable")}
                      />
                    ) : null}
                  </span>
                  <span className="mt-0.5 block truncate text-caption text-content-muted">
                    {session.preview ?? t(`sessions.tool.${session.tool}`)}
                  </span>
                  <span className="mt-2 flex flex-wrap gap-x-3 gap-y-1 text-caption text-content-muted">
                    {session.projectName ? (
                      <span className="inline-flex min-w-0 items-center gap-1">
                        <Folder
                          className="h-3.5 w-3.5 shrink-0"
                          aria-hidden="true"
                        />
                        <span className="truncate">{session.projectName}</span>
                      </span>
                    ) : null}
                    {time ? (
                      <span className="inline-flex items-center gap-1">
                        <Clock3 className="h-3.5 w-3.5" aria-hidden="true" />
                        {time}
                      </span>
                    ) : null}
                  </span>
                </span>
              </button>
            </li>
          );
        })}
        {remaining > 0 ? (
          <li className="px-2 py-3 text-center">
            <Button
              size="sm"
              variant="ghost"
              onClick={() =>
                setVisibleCount((count) => count + LOAD_MORE_ITEMS)
              }
            >
              {t("sessions.list.loadMore", { count: nextCount })}
            </Button>
          </li>
        ) : null}
      </ul>
    </Card>
  );
}
