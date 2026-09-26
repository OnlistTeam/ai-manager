import {
  extensionScopeKey,
  type Extension,
  type ExtensionKind,
} from "@/entities/extension";

/**
 * One Skill or MCP connection across every app that can use it.
 *
 * The native list answers per app ("what does Claude Code have"), while the
 * page answers per item ("where is this one on"). Managed rows are global
 * records with a switch per app, so the same id in two lists is the same row.
 * A found-only item is keyed apart from a managed one: the two can share an
 * id without being the same thing.
 */
export interface UnifiedExtensionRow {
  key: string;
  id: string;
  kind: ExtensionKind;
  name: string;
  description: string | null;
  management: Extension["management"];
  /** This row's entry in each app that lists it, keyed by `extensionScopeKey`. */
  entries: ReadonlyMap<string, Extension>;
}

export function unifiedExtensionRows(
  lists: readonly (readonly Extension[])[],
): UnifiedExtensionRow[] {
  const rows = new Map<
    string,
    UnifiedExtensionRow & { entries: Map<string, Extension> }
  >();
  for (const list of lists) {
    for (const entry of list) {
      const key = `${entry.management}:${entry.id}`;
      const row = rows.get(key) ?? {
        key,
        id: entry.id,
        kind: entry.kind,
        name: entry.name,
        description: entry.description,
        management: entry.management,
        entries: new Map<string, Extension>(),
      };
      row.entries.set(extensionScopeKey(entry.scope), entry);
      rows.set(key, row);
    }
  }
  // One order by name, the order the native lists already use, so a found
  // item keeps its place when its first switch takes it over.
  return [...rows.values()].sort(
    (left, right) =>
      left.name.localeCompare(right.name) || left.id.localeCompare(right.id),
  );
}

/** Any one of the row's entries, for flows that act on the item as a whole. */
export function representativeEntry(row: UnifiedExtensionRow): Extension {
  const [first] = row.entries.values();
  if (first === undefined) {
    // Rows are only ever created from an entry.
    throw new Error(`extension row ${row.key} has no entry`);
  }
  return first;
}
