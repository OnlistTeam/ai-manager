import { Cable, MessageSquareText, WandSparkles } from "lucide-react";
import type { ExtensionKind } from "@/native/schemas/extension";
import { cn } from "./cn";

const ICON_BY_KIND = {
  skill: WandSparkles,
  mcp: Cable,
  prompt: MessageSquareText,
} satisfies Record<ExtensionKind, typeof WandSparkles>;

export interface ExtensionArtworkProps {
  kind: ExtensionKind;
  active?: boolean;
  className?: string;
}

/**
 * Extension identity is category-owned rather than vendor-owned. The product
 * model deliberately carries no remote icon or configuration payload, so the
 * artwork stays local, stable and honest for every extension in a category.
 */
export function ExtensionArtwork({
  kind,
  active = false,
  className,
}: ExtensionArtworkProps) {
  const Icon = ICON_BY_KIND[kind];

  return (
    <span
      aria-hidden="true"
      data-extension-artwork={kind}
      className={cn(
        "relative flex h-12 w-12 shrink-0 items-center justify-center overflow-hidden rounded-xl border shadow-sm",
        active
          ? "border-brand/25 bg-brand/10 text-brand"
          : "border-hairline bg-layer-1 text-content-muted",
        className,
      )}
    >
      <span className="absolute inset-1 rounded-lg bg-brand/5" />
      <Icon className="relative h-5 w-5" />
    </span>
  );
}
