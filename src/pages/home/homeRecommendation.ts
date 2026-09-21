import type { ToolId } from "@/entities/tool";
import type {
  QuickCheckItem,
  QuickCheckItemKind,
  QuickCheckSummary,
} from "@/features/health";

export type HomeDestination = "tools" | "services" | "extensions";

export interface HomeRecommendation {
  item: QuickCheckItem;
  toolId: ToolId | null;
  destination: HomeDestination;
  actionKey:
    | "home.card.actions.tools"
    | "home.card.actions.services"
    | "home.card.actions.extensions";
}

const DESTINATION: Record<QuickCheckItemKind, HomeDestination> = {
  tool: "tools",
  provider: "services",
  connectivity: "services",
  config: "services",
  mcp: "extensions",
};

const ACTION_KEY: Record<HomeDestination, HomeRecommendation["actionKey"]> = {
  tools: "home.card.actions.tools",
  services: "home.card.actions.services",
  extensions: "home.card.actions.extensions",
};

/**
 * Within the same severity, prefer the item that most affects a beginner's
 * ability to start using an installed tool. A missing service therefore comes
 * before an optional update, while a broken tool still beats every warning.
 */
const KIND_PRIORITY: Record<
  "action" | "attention",
  Record<QuickCheckItemKind, number>
> = {
  action: {
    tool: 0,
    config: 1,
    provider: 2,
    connectivity: 3,
    mcp: 4,
  },
  attention: {
    provider: 0,
    connectivity: 1,
    tool: 2,
    config: 3,
    mcp: 4,
  },
};

function recommendationRank(item: QuickCheckItem): number | null {
  if (item.status !== "action" && item.status !== "attention") return null;
  const severity = item.status === "action" ? 0 : 100;
  return severity + KIND_PRIORITY[item.status][item.kind];
}

/** Selects one honest next step from the same Quick Check data the Home health card shows. */
export function recommendHomeAction(
  summary: QuickCheckSummary,
): HomeRecommendation | null {
  if (summary.installedCount === 0) return null;

  let selected: QuickCheckItem | null = null;
  let selectedRank = Number.POSITIVE_INFINITY;

  for (const item of summary.items) {
    const rank = recommendationRank(item);
    if (rank !== null && rank < selectedRank) {
      selected = item;
      selectedRank = rank;
    }
  }

  if (selected === null) return null;
  const destination = DESTINATION[selected.kind];
  return {
    item: selected,
    toolId: selected.toolId,
    destination,
    actionKey: ACTION_KEY[destination],
  };
}
