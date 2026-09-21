import type { DesktopAppId } from "@/native/schemas/desktopApp";
import CherryStudioPng from "@/assets/icons/cherry-studio.png";
import CursorSvg from "@/assets/icons/cursor.svg?url";
import ZCodePng from "@/assets/icons/zcode.png";
import ClaudeSvg from "@/icons/extracted/claude.svg?url";
import OpenAISvg from "@/icons/extracted/openai.svg?url";
import { cn } from "./cn";

interface ArtworkMeta {
  src: string;
  imageClassName: string;
}

const DESKTOP_APP_ARTWORK: Record<DesktopAppId, ArtworkMeta> = {
  "codex-app": {
    src: OpenAISvg,
    imageClassName: "h-7 w-7 dark:invert",
  },
  "claude-desktop": {
    src: ClaudeSvg,
    imageClassName: "h-7 w-7",
  },
  cursor: {
    src: CursorSvg,
    imageClassName: "h-9 w-9 rounded-lg",
  },
  zcode: {
    src: ZCodePng,
    imageClassName: "h-9 w-9 rounded-lg",
  },
  "cherry-studio": {
    src: CherryStudioPng,
    imageClassName: "h-9 w-9 rounded-lg",
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
        "relative flex h-11 w-11 shrink-0 items-center justify-center overflow-hidden rounded-lg border border-hairline bg-layer-1 shadow-sm",
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
