import type { Extension } from "@/entities/extension";
import type { ToolId } from "@/entities/tool";

export interface LocalExtensionGroup {
  key: string;
  item: Extension;
  tools: ToolId[];
}

/** Only Skills have a proven shared-directory identity across tools. */
function groupKey(item: Extension): string {
  return item.kind === "skill"
    ? `skill:${item.id}`
    : `mcp:${item.scope.id}:${item.id}`;
}

export function groupLocalExtensionItems(
  items: readonly Extension[],
): LocalExtensionGroup[] {
  const groups = new Map<string, LocalExtensionGroup>();
  for (const item of items) {
    // The local-inventory schema guarantees tool scopes; retain the guard so
    // this pure helper also fails closed when called with unparsed data.
    if (item.scope.kind !== "tool") continue;
    const key = groupKey(item);
    const current = groups.get(key);
    if (current) {
      if (!current.tools.includes(item.scope.id))
        current.tools.push(item.scope.id);
    } else {
      groups.set(key, { key, item, tools: [item.scope.id] });
    }
  }
  return [...groups.values()].sort(
    (left, right) =>
      left.item.kind.localeCompare(right.item.kind) ||
      left.item.name.localeCompare(right.item.name),
  );
}

export interface SkillOwner {
  tool: ToolId;
  /** The tool's display name; falls back to its id if the inventory contains a tool not in the tools list. */
  name: string;
}

/**
 * Who else on this Mac owns this skill, indexed by skill id.
 *
 * The ownership row on the card only cares about names, while the copy
 * dialog needs to tell by ToolId which tool already has it, so both are
 * computed in one pass. When the inventory contains a tool not in the
 * current tools list, its raw id is kept — better to show something a bit
 * rough than silently drop a copy that genuinely exists.
 */
export function skillOwnersById(
  items: readonly Extension[],
  toolNames: ReadonlyMap<ToolId, string>,
): Map<string, SkillOwner[]> {
  const owners = new Map<string, SkillOwner[]>();
  for (const group of groupLocalExtensionItems(items)) {
    if (group.item.kind !== "skill") continue;
    owners.set(
      group.item.id,
      group.tools.map((tool) => ({ tool, name: toolNames.get(tool) ?? tool })),
    );
  }
  return owners;
}
