import { useDiscoverIcon } from "@/entities/discover";
import { cn } from "@/shared/ui/cn";

export interface DiscoverLogoProps {
  url: string | null;
  name: string;
  size?: "md" | "lg";
}

function initialOf(name: string): string {
  const letter = name.replace(/^[^\p{L}\p{N}]+/u, "").charAt(0);
  return letter ? letter.toUpperCase() : "?";
}

/**
 * The project's own logo, or its owner's avatar, fetched natively because
 * the window only shows pictures it was handed as data. Until one arrives,
 * and when there is none, the name's initial sits on a tile.
 */
export function DiscoverLogo({ url, name, size = "md" }: DiscoverLogoProps) {
  const icon = useDiscoverIcon(url);
  const box = cn(
    "flex shrink-0 items-center justify-center overflow-hidden rounded-md",
    size === "lg" ? "h-10 w-10" : "h-8 w-8",
  );

  if (icon.data) {
    const vector = icon.data.startsWith("data:image/svg+xml");
    return (
      <span className={cn(box, "bg-layer-1 ring-1 ring-inset ring-hairline")}>
        <img
          src={icon.data}
          alt=""
          decoding="async"
          className={
            vector
              ? "h-[72%] w-[72%] object-contain"
              : "h-full w-full object-cover"
          }
        />
      </span>
    );
  }

  return (
    <span
      aria-hidden="true"
      className={cn(
        box,
        "bg-layer-2 font-semibold text-content-muted",
        size === "lg" ? "text-heading" : "text-body",
      )}
    >
      {initialOf(name)}
    </span>
  );
}
