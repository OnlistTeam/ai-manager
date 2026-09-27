import { useId, useRef } from "react";
import type { ExtensionScopeOption } from "@/features/extension-management";
import { TooltipProvider } from "@/shared/ui/Tooltip";
import { McpDiscover } from "./McpDiscover";
import { SkillDiscover } from "./SkillDiscover";
import { useDiscoverSearch } from "./useDiscoverSearch";
import { useFirstSight } from "./useFirstSight";

export interface DiscoverSectionProps {
  kind: "skill" | "mcp";
  /** The page's apps that take this kind. */
  targets: readonly ExtensionScopeOption[];
  /** Adding is paused, as the list's own actions are. */
  blocked: boolean;
}

/**
 * Discover, at the bottom of the Skills and MCP pages (ADR-0063): things to
 * add, found in the product's featured list, the MCP Registry or skills.sh.
 * It asks its sources only once it scrolls into view, so the list above never
 * waits for it.
 */
export function DiscoverSection({
  kind,
  targets,
  blocked,
}: DiscoverSectionProps) {
  const ref = useRef<HTMLElement>(null);
  const visible = useFirstSight(ref);
  // One letter would match most of skills.sh; wait for a second.
  const search = useDiscoverSearch(kind === "skill" ? 2 : 1);
  const headingId = useId();
  const props = { headingId, search, visible, targets, blocked };

  return (
    <section
      ref={ref}
      aria-labelledby={headingId}
      className="flex min-w-0 flex-col gap-3 border-t border-hairline pt-4"
    >
      <TooltipProvider delayDuration={200} skipDelayDuration={100}>
        {kind === "mcp" ? (
          <McpDiscover {...props} />
        ) : (
          <SkillDiscover {...props} />
        )}
      </TooltipProvider>
    </section>
  );
}
