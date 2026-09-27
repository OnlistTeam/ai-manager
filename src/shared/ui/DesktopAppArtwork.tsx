import type { DesktopAppId } from "@/native/schemas/desktopApp";
import CherryStudioPng from "@/assets/icons/cherry-studio.png";
import CursorSvg from "@/assets/icons/cursor.svg?url";
import ZCodePng from "@/assets/icons/zcode.png";
import ClaudeSvg from "@/icons/extracted/claude.svg?url";
import LmStudioSvg from "@/icons/extracted/lmstudio.svg?url";
import OllamaSvg from "@/icons/extracted/ollama.svg?url";
import OpenAISvg from "@/icons/extracted/openai.svg?url";
import { cn } from "./cn";

interface ArtworkMeta {
  src: string;
  imageClassName: string;
  /** A word or two that fits a narrow column header, e.g. above a switch. */
  shortName: string;
}

const DESKTOP_APP_ARTWORK: Record<DesktopAppId, ArtworkMeta> = {
  "codex-app": {
    src: OpenAISvg,
    imageClassName: "h-7 w-7 dark:invert",
    shortName: "Codex",
  },
  "claude-desktop": {
    src: ClaudeSvg,
    imageClassName: "h-7 w-7",
    shortName: "Claude",
  },
  cursor: {
    src: CursorSvg,
    imageClassName: "h-9 w-9 rounded-lg",
    shortName: "Cursor",
  },
  zcode: {
    src: ZCodePng,
    imageClassName: "h-9 w-9 rounded-lg",
    shortName: "ZCode",
  },
  "cherry-studio": {
    src: CherryStudioPng,
    imageClassName: "h-9 w-9 rounded-lg",
    shortName: "Cherry",
  },
  // Both marks are single-colour, drawn like the OpenAI one.
  "lm-studio": {
    src: LmStudioSvg,
    imageClassName: "h-7 w-7 dark:invert",
    shortName: "LM Studio",
  },
  ollama: {
    src: OllamaSvg,
    imageClassName: "h-7 w-7 dark:invert",
    shortName: "Ollama",
  },
};

export interface DesktopAppArtworkProps {
  appId: DesktopAppId;
  className?: string;
}

export function DesktopAppArtwork({
  appId,
  className,
}: DesktopAppArtworkProps) {
  const artwork = DESKTOP_APP_ARTWORK[appId];

  return (
    <span
      aria-hidden="true"
      data-desktop-app-artwork={appId}
      className={cn(
        "relative flex h-9 w-9 shrink-0 items-center justify-center overflow-hidden rounded-lg border border-hairline bg-layer-1 shadow-sm",
        className,
      )}
    >
      <span className="absolute inset-1 rounded-md bg-brand/5" />
      <img
        src={artwork.src}
        alt=""
        draggable={false}
        className={cn("relative object-contain", artwork.imageClassName)}
      />
    </span>
  );
}

/** The bare mark at icon size; see `ToolGlyph`. */
export function DesktopAppGlyph({ appId, className }: DesktopAppArtworkProps) {
  const artwork = DESKTOP_APP_ARTWORK[appId];
  return (
    <img
      src={artwork.src}
      alt=""
      aria-hidden="true"
      draggable={false}
      className={cn(
        "object-contain",
        artwork.imageClassName,
        "h-4 w-4",
        className,
      )}
    />
  );
}

/** The short name for a narrow column; the glyph carries the desktop badge. */
export function desktopAppShortName(id: DesktopAppId): string {
  return DESKTOP_APP_ARTWORK[id].shortName;
}
