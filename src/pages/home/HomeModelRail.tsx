import { LayoutGrid, Star } from "lucide-react";
import type { ReactNode } from "react";
import { useTranslation } from "react-i18next";
import type { ToolId } from "@/entities/tool";
import { cn } from "@/shared/ui/cn";
import { ServiceMark } from "@/shared/ui/ServiceArtwork";
import { ToolGlyph } from "@/shared/ui/ToolArtwork";
import type {
  ModelMenuEndpoint,
  ModelMenuSection,
  ModelMenuView,
} from "./homeModelMenu";

/** The endpoint's logo or initials; the tool's own mark for its sign-in. */
export function EndpointMark({
  endpoint,
  tool,
  className,
}: {
  endpoint: ModelMenuEndpoint;
  tool: ToolId;
  className?: string;
}) {
  return endpoint.signIn ? (
    <ToolGlyph toolId={tool} className={cn("h-4 w-4 shrink-0", className)} />
  ) : (
    <ServiceMark
      provider={endpoint.provider}
      name={endpoint.name}
      className={className}
    />
  );
}

export interface HomeModelRailProps {
  tool: ToolId;
  sections: readonly ModelMenuSection[];
  view: ModelMenuView;
  onView: (view: ModelMenuView) => void;
}

/** All models, the starred ones, then one button per endpoint. */
export function HomeModelRail({
  tool,
  sections,
  view,
  onView,
}: HomeModelRailProps) {
  const { t } = useTranslation();
  const railButton = (
    key: string,
    label: string,
    on: boolean,
    next: ModelMenuView,
    icon: ReactNode,
  ) => (
    <button
      key={key}
      type="button"
      title={label}
      aria-label={label}
      aria-pressed={on}
      onClick={() => onView(next)}
      className={cn(
        "flex h-8 w-8 shrink-0 items-center justify-center rounded-md text-content-muted transition-colors duration-fast hover:bg-layer-2 hover:text-content",
        on && "bg-layer-2 text-content",
      )}
    >
      {icon}
    </button>
  );

  return (
    <nav
      aria-label={t("home.model.views")}
      className="flex w-11 shrink-0 flex-col items-center gap-1 overflow-y-auto border-r border-hairline py-2"
    >
      {railButton(
        "all",
        t("home.model.all"),
        view === "all",
        "all",
        <LayoutGrid className="h-4 w-4" aria-hidden="true" />,
      )}
      {railButton(
        "favorites",
        t("home.model.favorites"),
        view === "favorites",
        "favorites",
        <Star className="h-4 w-4" aria-hidden="true" />,
      )}
      <span className="my-1 h-px w-5 shrink-0 bg-hairline" />
      {sections.map(({ endpoint }) =>
        railButton(
          endpoint.key,
          endpoint.name,
          typeof view === "object" && view.endpoint === endpoint.key,
          { endpoint: endpoint.key },
          <EndpointMark endpoint={endpoint} tool={tool} />,
        ),
      )}
    </nav>
  );
}
