import AnthropicSvg from "@/icons/extracted/anthropic.svg?url";
import AwsSvg from "@/icons/extracted/aws.svg?url";
import ClaudeSvg from "@/icons/extracted/claude.svg?url";
import DeepSeekSvg from "@/icons/extracted/deepseek.svg?url";
import GeminiSvg from "@/icons/extracted/gemini.svg?url";
import GrokSvg from "@/icons/extracted/grok.svg?url";
import GroqSvg from "@/icons/extracted/groq.svg?url";
import HermesPng from "@/icons/extracted/hermes.png";
import KimiSvg from "@/icons/extracted/kimi.svg?url";
import LmStudioSvg from "@/icons/extracted/lmstudio.svg?url";
import MiniMaxSvg from "@/icons/extracted/minimax.svg?url";
import MistralSvg from "@/icons/extracted/mistral.svg?url";
import OllamaSvg from "@/icons/extracted/ollama.svg?url";
import OpenAISvg from "@/icons/extracted/openai.svg?url";
import OpenCodeSvg from "@/icons/extracted/opencode-logo-light.svg?url";
import OpenRouterSvg from "@/icons/extracted/openrouter.svg?url";
import QwenSvg from "@/icons/extracted/qwen.svg?url";
import StepFunSvg from "@/icons/extracted/stepfun.svg?url";
import XaiSvg from "@/icons/extracted/xai.svg?url";
import ZaiSvg from "@/icons/extracted/zai.svg?url";
import ZhipuSvg from "@/icons/extracted/zhipu.svg?url";
import type { Provider, ToolLoginAccount } from "@/native/schemas/provider";
import type { ToolId } from "@/native/schemas/tool";
import { cn } from "./cn";

type ArtworkId =
  | "anthropic"
  | "aws"
  | "claude"
  | "deepseek"
  | "gemini"
  | "grok"
  | "groq"
  | "kimi"
  | "lmstudio"
  | "minimax"
  | "mistral"
  | "nous"
  | "ollama"
  | "openai"
  | "opencode"
  | "openrouter"
  | "qwen"
  | "stepfun"
  | "xai"
  | "zai"
  | "zhipu"
  | "generic";

interface BrandedArtwork {
  id: Exclude<ArtworkId, "generic">;
  src: string;
  /** Size at full scale. */
  imageClassName: string;
  /** A one-colour logo, inverted in dark mode. */
  mono: boolean;
}

const ARTWORK: Record<BrandedArtwork["id"], BrandedArtwork> = {
  anthropic: {
    id: "anthropic",
    mono: true,
    src: AnthropicSvg,
    imageClassName: "h-7 w-7",
  },
  openai: {
    id: "openai",
    mono: true,
    src: OpenAISvg,
    imageClassName: "h-7 w-7",
  },
  openrouter: {
    id: "openrouter",
    mono: true,
    src: OpenRouterSvg,
    imageClassName: "h-7 w-7",
  },
  gemini: {
    id: "gemini",
    mono: false,
    src: GeminiSvg,
    imageClassName: "h-7 w-7",
  },
  opencode: {
    id: "opencode",
    mono: true,
    src: OpenCodeSvg,
    imageClassName: "h-8 w-7",
  },
  aws: { id: "aws", mono: true, src: AwsSvg, imageClassName: "h-7 w-7" },
  claude: {
    id: "claude",
    mono: false,
    src: ClaudeSvg,
    imageClassName: "h-7 w-7",
  },
  deepseek: {
    id: "deepseek",
    mono: false,
    src: DeepSeekSvg,
    imageClassName: "h-7 w-7",
  },
  grok: { id: "grok", mono: true, src: GrokSvg, imageClassName: "h-7 w-7" },
  groq: { id: "groq", mono: true, src: GroqSvg, imageClassName: "h-7 w-7" },
  kimi: { id: "kimi", mono: true, src: KimiSvg, imageClassName: "h-7 w-7" },
  lmstudio: {
    id: "lmstudio",
    mono: true,
    src: LmStudioSvg,
    imageClassName: "h-7 w-7",
  },
  minimax: {
    id: "minimax",
    mono: false,
    src: MiniMaxSvg,
    imageClassName: "h-7 w-7",
  },
  mistral: {
    id: "mistral",
    mono: false,
    src: MistralSvg,
    imageClassName: "h-7 w-7",
  },
  nous: {
    id: "nous",
    mono: false,
    src: HermesPng,
    imageClassName: "h-7 w-7 rounded-md",
  },
  ollama: {
    id: "ollama",
    mono: true,
    src: OllamaSvg,
    imageClassName: "h-7 w-7",
  },
  qwen: { id: "qwen", mono: false, src: QwenSvg, imageClassName: "h-7 w-7" },
  stepfun: {
    id: "stepfun",
    mono: false,
    src: StepFunSvg,
    imageClassName: "h-7 w-7",
  },
  xai: { id: "xai", mono: true, src: XaiSvg, imageClassName: "h-7 w-7" },
  zai: { id: "zai", mono: true, src: ZaiSvg, imageClassName: "h-7 w-7" },
  zhipu: {
    id: "zhipu",
    mono: false,
    src: ZhipuSvg,
    imageClassName: "h-7 w-7",
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
  ["deepseek.com", "deepseek"],
  ["moonshot.ai", "kimi"],
  ["moonshot.cn", "kimi"],
  ["kimi.com", "kimi"],
  ["kimi.ai", "kimi"],
  ["z.ai", "zai"],
  ["bigmodel.cn", "zhipu"],
  ["minimax.io", "minimax"],
  ["minimaxi.com", "minimax"],
  ["stepfun.ai", "stepfun"],
  ["stepfun.com", "stepfun"],
  ["aliyuncs.com", "qwen"],
  ["alibabacloud.com", "qwen"],
  ["aliyun.com", "qwen"],
  ["mistral.ai", "mistral"],
  ["groq.com", "groq"],
  ["ollama.com", "ollama"],
  ["lmstudio.ai", "lmstudio"],
  ["x.ai", "xai"],
  ["opencode.ai", "opencode"],
  ["nousresearch.com", "nous"],
  ["amazonaws.com", "aws"],
  ["aws.amazon.com", "aws"],
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

function artworkForUrls(
  urls: ReadonlyArray<string | null>,
): BrandedArtwork | null {
  for (const candidate of urls.map(hostname)) {
    if (!candidate) continue;
    const matched = DOMAIN_IDENTITY.find(([domain]) =>
      matchesDomain(candidate, domain),
    );
    if (matched) return ARTWORK[matched[1]];
  }
  return null;
}

function resolveArtwork(provider: Provider): BrandedArtwork | null {
  if (provider.kind === "official") {
    const identity = OFFICIAL_BY_TOOL[provider.tool];
    return identity ? ARTWORK[identity] : null;
  }

  const byDomain = artworkForUrls([provider.websiteUrl, provider.baseUrl]);
  if (byDomain) return byDomain;

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
          className={cn(
            "relative object-contain",
            artwork.imageClassName,
            artwork.mono && "dark:invert",
          )}
        />
      ) : (
        <span className="relative text-caption font-semibold tracking-wide text-brand">
          {initials(provider.name)}
        </span>
      )}
    </span>
  );
}

export interface ServiceMarkProps {
  /** The saved entry; without one, the name's initials stand in. */
  provider: Provider | null;
  name: string;
  className?: string;
}

/**
 * The same identity at text size, for a list row or a compact button: the
 * recognised logo, else the initials.
 */
export function ServiceMark({ provider, name, className }: ServiceMarkProps) {
  const artwork = provider ? resolveArtwork(provider) : null;

  return (
    <span
      aria-hidden="true"
      data-service-artwork={artwork?.id ?? "generic"}
      className={cn(
        "flex h-4 w-4 shrink-0 items-center justify-center overflow-hidden rounded-[4px]",
        !artwork && "bg-brand/10",
        className,
      )}
    >
      {artwork ? (
        <img
          src={artwork.src}
          alt=""
          draggable={false}
          className={cn(
            "h-full w-full object-contain",
            artwork.mono && "dark:invert",
          )}
        />
      ) : (
        <span className="text-[8px] font-semibold leading-none text-brand">
          {initials(name)}
        </span>
      )}
    </span>
  );
}

/** The account each tool signs in with, drawn as that account's own mark. */
const TOOL_LOGIN_ARTWORK: Record<ToolLoginAccount, BrandedArtwork["id"]> = {
  claude: "claude",
  chatGpt: "openai",
  google: "gemini",
  superGrok: "grok",
};

export interface ServiceLogoProps {
  name: string;
  /** Addresses that identify the service: its website first, then its API. */
  urls?: ReadonlyArray<string | null>;
  /** A tool's own sign-in, which has no address to go by. */
  account?: ToolLoginAccount;
  className?: string;
}

/**
 * A catalogue entry's mark on the add page, before any endpoint is saved from
 * it. Same rule as a saved card: a logo only on a domain match, else the
 * name's initials.
 */
export function ServiceLogo({
  name,
  urls = [],
  account,
  className,
}: ServiceLogoProps) {
  const artwork = account
    ? ARTWORK[TOOL_LOGIN_ARTWORK[account]]
    : artworkForUrls(urls);

  return (
    <span
      aria-hidden="true"
      data-service-artwork={artwork?.id ?? "generic"}
      className={cn(
        "flex h-5 w-5 shrink-0 items-center justify-center overflow-hidden rounded-[5px]",
        !artwork && "bg-brand/10",
        className,
      )}
    >
      {artwork ? (
        <img
          src={artwork.src}
          alt=""
          draggable={false}
          className={cn(
            "h-full w-full object-contain",
            artwork.mono && "dark:invert",
          )}
        />
      ) : (
        <span className="text-[9px] font-semibold leading-none text-brand">
          {initials(name)}
        </span>
      )}
    </span>
  );
}
