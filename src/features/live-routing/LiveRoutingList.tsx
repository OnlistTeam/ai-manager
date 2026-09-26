import { useState } from "react";
import { useTranslation } from "react-i18next";
import type { RoutingTraceEntry } from "@/entities/routing";
import { LiveRoutingRow } from "./LiveRoutingRow";

/**
 * Newest request on top. Rows that arrive after the list first painted fade
 * in; the seed does not, so opening the page is calm.
 */
export function LiveRoutingList({
  entries,
  hideEmails,
  compact,
}: {
  entries: readonly RoutingTraceEntry[];
  hideEmails: boolean;
  compact: boolean;
}) {
  const { t } = useTranslation();
  const [seededUpTo] = useState(() =>
    entries.reduce((highest, entry) => Math.max(highest, entry.seq), 0),
  );

  return (
    <ol
      aria-label={t("routing.live.listLabel")}
      className="max-h-80 overflow-y-auto border-t border-hairline"
    >
      {entries.map((entry) => (
        <LiveRoutingRow
          key={entry.seq}
          entry={entry}
          fresh={entry.seq > seededUpTo}
          hideEmails={hideEmails}
          compact={compact}
        />
      ))}
    </ol>
  );
}
