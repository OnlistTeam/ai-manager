import {
  Boxes,
  MessagesSquare,
  MessageSquareText,
  Plug,
  Home,
  Puzzle,
  Settings,
  Waypoints,
  type LucideIcon,
} from "lucide-react";

/** Primary application routes, in sidebar order. */
export const APP_ROUTES = [
  "home",
  "tools",
  "services",
  "extensions",
  "mcp",
  "prompts",
  "data",
  "settings",
] as const;

export type AppRoute = (typeof APP_ROUTES)[number];

export const DEFAULT_ROUTE: AppRoute = "home";

export interface NavItem {
  id: AppRoute;
  labelKey: string;
  icon: LucideIcon;
  /** Footer items stay pinned to the bottom of the rail. */
  placement?: "footer";
}

/**
 * Task-oriented destinations (ADR-0038). Keep extensions/data route IDs for
 * existing navigation memory; they now mean Skills/Sessions respectively.
 */
export const NAV_ITEMS: readonly NavItem[] = [
  { id: "home", labelKey: "nav.home", icon: Home },
  { id: "tools", labelKey: "nav.tools", icon: Boxes },
  { id: "services", labelKey: "nav.services", icon: Waypoints },
  { id: "extensions", labelKey: "nav.extensions", icon: Puzzle },
  { id: "mcp", labelKey: "nav.mcp", icon: Plug },
  { id: "prompts", labelKey: "nav.prompts", icon: MessageSquareText },
  { id: "data", labelKey: "nav.data", icon: MessagesSquare },
  {
    id: "settings",
    labelKey: "nav.settings",
    icon: Settings,
    placement: "footer",
  },
];

export function isAppRoute(value: unknown): value is AppRoute {
  return (
    typeof value === "string" &&
    (APP_ROUTES as readonly string[]).includes(value)
  );
}
