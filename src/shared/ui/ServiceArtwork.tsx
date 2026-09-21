import AnthropicSvg from "@/icons/extracted/anthropic.svg?url";
import GeminiSvg from "@/icons/extracted/gemini.svg?url";
import OpenAISvg from "@/icons/extracted/openai.svg?url";
import OpenCodeSvg from "@/icons/extracted/opencode-logo-light.svg?url";
import OpenRouterSvg from "@/icons/extracted/openrouter.svg?url";
import type { Provider } from "@/native/schemas/provider";
import type { ToolId } from "@/native/schemas/tool";
import { cn } from "./cn";

type ArtworkId =
  | "anthropic"
  | "openai"
  | "openrouter"
  | "gemini"
  | "opencode"
  | "generic";

interface BrandedArtwork {
  id: Exclude<ArtworkId, "generic">;
  src: string;
  imageClassName: string;
}

const ARTWORK: Record<BrandedArtwork["id"], BrandedArtwork> = {
  anthropic: {
    id: "anthropic",
    src: AnthropicSvg,
    imageClassName: "h-7 w-7 dark:invert",
  },
  openai: {
    id: "openai",
    src: OpenAISvg,
    imageClassName: "h-7 w-7 dark:invert",
  },
  openrouter: {
    id: "openrouter",
    src: OpenRouterSvg,
    imageClassName: "h-7 w-7 dark:invert",
  },
  gemini: {
    id: "gemini",
    src: GeminiSvg,
    imageClassName: "h-7 w-7",
  },
  opencode: {
    id: "opencode",
    src: OpenCodeSvg,
    imageClassName: "h-8 w-7 dark:invert",
  },
};

const OFFICIAL_BY_TOOL: Partial<Record<ToolId, BrandedArtwork["id"]>> = {
  "claude-code": "anthropic",
  codex: "openai",
  opencode: "opencode",
  "gemini-cli": "gemini",
};

const EXACT_IDENTITY: Record<string, BrandedArtwork["id"]> = {
  anthropic: "anthropic",
  "claude official": "anthropic",
  openai: "openai",
  "openai official": "openai",
  openrouter: "openrouter",
  gemini: "gemini",
  "google official": "gemini",
};

const DOMAIN_IDENTITY: ReadonlyArray<
  readonly [domain: string, artwork: BrandedArtwork["id"]]
> = [
  ["anthropic.com", "anthropic"],
  ["openai.com", "openai"],
  ["chatgpt.com", "openai"],
  ["openrouter.ai", "openrouter"],
  ["gemini.google.com", "gemini"],
  ["ai.google.dev", "gemini"],
  ["generativelanguage.googleapis.com", "gemini"],
];

function hostname(value: string | null): string | null {
  if (!value) return null;
  try {
    return new URL(value).hostname.toLowerCase();
  } catch {
    return null;
  }
}

function matchesDomain(candidate: string, domain: string): boolean {
  return candidate === domain || candidate.endsWith(`.${domain}`);
}

function resolveArtwork(provider: Provider): BrandedArtwork | null {
  if (provider.kind === "official") {
    const identity = OFFICIAL_BY_TOOL[provider.tool];
    return identity ? ARTWORK[identity] : null;
  }

  for (const candidate of [
    hostname(provider.websiteUrl),
    hostname(provider.baseUrl),
  ]) {
    if (!candidate) continue;
    const matched = DOMAIN_IDENTITY.find(([domain]) =>
      matchesDomain(candidate, domain),
    );
    if (matched) return ARTWORK[matched[1]];
  }

  for (const candidate of [provider.id, provider.name]) {
    const matched = EXACT_IDENTITY[candidate.trim().toLowerCase()];
    if (matched) return ARTWORK[matched];
  }
  return null;
}

function initials(name: string): string {
  const words = name.trim().split(/\s+/).filter(Boolean);
  if (words.length === 0) return "AI";
  if (words.length === 1) return words[0].slice(0, 2).toUpperCase();
  return `${words[0][0]}${words[1][0]}`.toUpperCase();
}

export interface ServiceArtworkProps {
  provider: Provider;
  className?: string;
}

/**
 * Recognized first-party identity is deliberately conservative: official
 * services use their owning tool, while custom services need an exact name or
 * domain match. Unknown relays keep their own initials instead of borrowing a
 * logo that could imply the wrong operator.
 */
export function ServiceArtwork({ provider, className }: ServiceArtworkProps) {
  const artwork = resolveArtwork(provider);

  return (
    <span
      aria-hidden="true"
      data-service-artwork={artwork?.id ?? "generic"}
      className={cn(
        "relative flex h-12 w-12 shrink-0 items-center justify-center overflow-hidden rounded-xl border border-hairline bg-layer-1 shadow-sm",
        className,
      )}
    >
      <span className="absolute inset-1 rounded-lg bg-brand/5" />
      {artwork ? (
        <img
          src={artwork.src}
          alt=""
          draggable={false}
          className={cn("relative object-contain", artwork.imageClassName)}
        />
      ) : (
        <span className="relative text-caption font-semibold tracking-wide text-brand">
          {initials(provider.name)}
        </span>
      )}
    </span>
  );
}
