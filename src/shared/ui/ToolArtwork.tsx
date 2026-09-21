import type { ToolId } from "@/native/schemas/tool";
import ClaudeSvg from "@/icons/extracted/claude.svg?url";
import DeepSeekSvg from "@/icons/extracted/deepseek.svg?url";
import OpenClawSvg from "@/icons/extracted/claw.svg?url";
import GeminiSvg from "@/icons/extracted/gemini.svg?url";
import GrokSvg from "@/icons/extracted/grok.svg?url";
import HermesPng from "@/icons/extracted/hermes.png";
import KimiSvg from "@/icons/extracted/kimi.svg?url";
import OpenAISvg from "@/icons/extracted/openai.svg?url";
import OpenCodeSvg from "@/icons/extracted/opencode-logo-light.svg?url";
import { getIcon } from "@/icons/extracted";
import { cn } from "./cn";

interface ArtworkMeta {
  src: string;
  imageClassName: string;
}

/**
 * Stable tool identity belongs in one presentation table. Tool lifecycle and
 * action logic remain status/capability-driven in ToolCard and ToolsPage.
 */
const TOOL_ARTWORK: Record<ToolId, ArtworkMeta> = {
  "claude-code": {
    src: ClaudeSvg,
    imageClassName: "h-7 w-7",
  },
  codex: {
    src: OpenAISvg,
    imageClassName: "h-7 w-7 dark:invert",
  },
  opencode: {
    src: OpenCodeSvg,
    imageClassName: "h-8 w-7 dark:invert",
  },
  "gemini-cli": {
    src: GeminiSvg,
    imageClassName: "h-7 w-7",
  },
  "grok-build": {
    src: GrokSvg,
    imageClassName: "h-7 w-7 dark:invert",
  },
  openclaw: {
    src: OpenClawSvg,
    imageClassName: "h-8 w-8",
  },
  hermes: {
    src: HermesPng,
    imageClassName: "h-8 w-8 rounded-md",
  },
  pi: {
    src: `data:image/svg+xml,${encodeURIComponent(getIcon("pi"))}`,
    imageClassName: "h-7 w-7 dark:invert",
  },
  "kimi-code": {
    src: KimiSvg,
    imageClassName: "h-7 w-7",
  },
  "deepseek-dsh": {
    src: DeepSeekSvg,
    imageClassName: "h-7 w-7",
  },
};

export interface ToolArtworkProps {
  toolId: ToolId;
  className?: string;
}

export function ToolArtwork({ toolId, className }: ToolArtworkProps) {
  const artwork = TOOL_ARTWORK[toolId];

  return (
    <span
      aria-hidden="true"
      data-tool-artwork={toolId}
      className={cn(
        "relative flex h-12 w-12 shrink-0 items-center justify-center overflow-hidden rounded-xl border border-hairline bg-layer-1 shadow-sm",
        className,
      )}
    >
      <span className="absolute inset-1 rounded-lg bg-brand/5" />
      <img
        src={artwork.src}
        alt=""
        draggable={false}
        className={cn("relative object-contain", artwork.imageClassName)}
      />
    </span>
  );
}
