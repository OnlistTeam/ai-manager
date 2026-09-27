import type { ReactNode } from "react";
import { ExternalLink } from "lucide-react";
import { Button } from "@/shared/ui/Button";
import { DiscoverLogo } from "./DiscoverLogo";

export interface DiscoverSheetHeadProps {
  icon: string | null;
  name: string;
  meta: ReactNode;
  links: readonly { label: string; onOpen: () => void }[];
}

/**
 * The logo, one line of facts and the item's pages, under the dialog's
 * title (which already carries the name).
 */
export function DiscoverSheetHead({
  icon,
  name,
  meta,
  links,
}: DiscoverSheetHeadProps) {
  return (
    <div className="flex min-w-0 items-center gap-3 border-b border-hairline pb-3">
      <DiscoverLogo url={icon} name={name} size="lg" />
      <div className="flex min-w-0 flex-1 flex-col gap-1">
        <span className="flex min-w-0 flex-wrap items-center gap-1.5 text-caption text-content-muted">
          {meta}
        </span>
        {links.length > 0 ? (
          <span className="-ml-2 flex flex-wrap gap-1">
            {links.map((link) => (
              <Button
                key={link.label}
                size="xs"
                variant="ghost"
                onClick={link.onOpen}
              >
                {link.label}
                <ExternalLink className="h-3 w-3" aria-hidden="true" />
              </Button>
            ))}
          </span>
        ) : null}
      </div>
    </div>
  );
}
