import type { ToolId } from "@/entities/tool";

/** Desktop-to-CLI handoffs focus the primary action, not the details link. */
export function focusToolCard(tool: ToolId): void {
  const card = document.querySelector<HTMLElement>(
    `[data-tool-card="${tool}"]`,
  );
  card?.scrollIntoView?.({ block: "center", behavior: "smooth" });
  const target =
    card?.querySelector<HTMLButtonElement>(
      "[data-tool-primary-action]:not(:disabled)",
    ) ?? card?.querySelector<HTMLButtonElement>("button:not(:disabled)");
  target?.focus();
}
