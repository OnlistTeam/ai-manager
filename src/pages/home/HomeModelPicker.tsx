import * as Popover from "@radix-ui/react-popover";
import { Command } from "cmdk";
import { AlertTriangle, ChevronDown, LoaderCircle, Search } from "lucide-react";
import { useState } from "react";
import { useTranslation } from "react-i18next";
import type { Tool } from "@/entities/tool";
import { Button } from "@/shared/ui/Button";
import { cn } from "@/shared/ui/cn";
import {
  HomeModelManage,
  HomeModelRows,
  HomeModelTyped,
} from "./HomeModelList";
import { EndpointMark, HomeModelRail } from "./HomeModelRail";
import {
  viewModelSections,
  type ModelMenuItem,
  type ModelMenuView,
} from "./homeModelMenu";
import { useHomeModelMenu } from "./useHomeModelMenu";
import type { HomeToolConnection } from "./useHomeToolConnection";
import { useModelFavorites } from "./useModelFavorites";

export interface HomeModelPickerProps {
  tool: Tool;
  connection: HomeToolConnection;
  onOpenServices: () => void;
}

/** Every model pill shares one width so the column lines up down the list. */
export const MODEL_PILL_CLASS =
  "h-7 w-56 shrink-0 rounded-full px-3 text-caption";

/**
 * The model a tool runs (ADR-0055), as a compact button led by its
 * endpoint's mark. The list is every endpoint's models under the endpoint's
 * name, the one in use first, with a rail to narrow it to the starred models
 * or one endpoint. Picking a model under another endpoint switches to that
 * endpoint first; a name typed into the filter can be used as typed.
 */
export function HomeModelPicker({
  tool,
  connection,
  onOpenServices,
}: HomeModelPickerProps) {
  const { t } = useTranslation();
  const [open, setOpen] = useState(false);
  const [query, setQuery] = useState("");
  const [view, setView] = useState<ModelMenuView>("all");
  const favorites = useModelFavorites(tool.id);
  const { sections, inUse } = useHomeModelMenu(
    tool,
    connection,
    connection.choice,
    open,
  );

  const shown = viewModelSections(sections, view, query, favorites.isFavorite);
  const typed = query.trim();
  const target =
    typeof view === "object"
      ? (sections.find((section) => section.endpoint.key === view.endpoint)
          ?.endpoint ?? null)
      : inUse;
  const offerTyped =
    typed.length > 0 &&
    target !== null &&
    !sections.some((section) =>
      section.items.some((item) => item.model === typed),
    );
  const message =
    sections.length === 0
      ? t("home.tools.noEndpoints")
      : shown.length === 0 && !offerTyped
        ? t(
            view === "favorites" && !typed
              ? "home.model.noFavorites"
              : "home.model.noMatch",
          )
        : null;

  const openChange = (next: boolean) => {
    setOpen(next);
    if (!next) {
      setQuery("");
      setView("all");
    }
  };
  const choose = (item: Pick<ModelMenuItem, "endpoint" | "model">) => {
    openChange(false);
    connection.chooseModel(item.endpoint.providerId, item.model);
  };

  const switching = connection.switchingName;
  const busy = switching !== null || connection.settingModel;
  const model = connection.model;
  const label = switching
    ? t("services.switch.switchingNamed", { name: switching })
    : (model ?? t("home.model.toolDefault"));
  const outranked =
    connection.connection.kind === "external" &&
    connection.connection.connection.outranksSwitch;
  const where = inUse
    ? [inUse.name, inUse.detail].filter(Boolean).join(" · ")
    : t("home.tools.notConnected");

  return (
    <Popover.Root open={open} onOpenChange={openChange}>
      <Popover.Trigger asChild>
        <Button
          variant="secondary"
          size="xs"
          disabled={connection.switchDisabled || connection.settingModel}
          title={inUse?.title ?? where}
          aria-label={t("home.model.pickNamed", {
            tool: tool.name,
            current: `${label}, ${where}`,
          })}
          className={cn(
            MODEL_PILL_CLASS,
            "justify-between",
            outranked && "border-warning/40 bg-warning/10 hover:bg-warning/15",
          )}
        >
          {busy ? (
            <LoaderCircle
              className="h-3.5 w-3.5 shrink-0 motion-safe:animate-spin"
              aria-hidden="true"
            />
          ) : outranked ? (
            <AlertTriangle
              className="h-3.5 w-3.5 shrink-0 text-warning"
              aria-hidden="true"
            />
          ) : inUse ? (
            <EndpointMark endpoint={inUse} tool={tool.id} />
          ) : null}
          <span
            className={cn(
              "min-w-0 flex-1 truncate text-left",
              model === null && !switching
                ? "text-content-muted"
                : "text-content",
            )}
          >
            {label}
          </span>
          <ChevronDown
            className="h-3.5 w-3.5 shrink-0 text-content-muted"
            aria-hidden="true"
          />
        </Button>
      </Popover.Trigger>
      <Popover.Portal>
        <Popover.Content
          align="end"
          sideOffset={6}
          collisionPadding={16}
          className={cn(
            "app-floating-menu z-[70] flex w-[30rem] max-w-[calc(100vw-2rem)] flex-col overflow-hidden rounded-lg border outline-none animate-ds-overlay-in",
            "h-[min(26rem,var(--radix-popover-content-available-height))]",
          )}
        >
          <Command
            label={t("home.model.pickerLabel", { tool: tool.name })}
            loop
            shouldFilter={false}
            className="flex min-h-0 flex-1 flex-col outline-none"
          >
            <div className="flex shrink-0 items-center gap-2 border-b border-hairline px-3">
              <Search
                className="h-4 w-4 shrink-0 text-content-muted"
                aria-hidden="true"
              />
              <Command.Input
                value={query}
                onValueChange={setQuery}
                placeholder={t("home.model.filter")}
                className="h-10 min-w-0 flex-1 bg-transparent text-caption text-content outline-none placeholder:text-content-muted"
              />
            </div>
            <div className="flex min-h-0 flex-1">
              {sections.length > 0 ? (
                <HomeModelRail
                  tool={tool.id}
                  sections={sections}
                  view={view}
                  onView={setView}
                />
              ) : null}
              <div className="flex min-w-0 flex-1 flex-col">
                {message ? (
                  <p className="px-4 pb-1 pt-2 text-caption text-content-muted">
                    {message}
                  </p>
                ) : null}
                <Command.List
                  label={t("home.model.pickerLabel", { tool: tool.name })}
                  className="scrollbar-subtle min-h-0 flex-1 overflow-y-auto overscroll-contain py-1"
                >
                  <HomeModelRows
                    sections={shown}
                    favorites={favorites}
                    loadingLabel={t("home.model.loading")}
                    onChoose={choose}
                  />
                  {offerTyped && target ? (
                    <HomeModelTyped
                      label={t("home.model.useTyped", { model: typed })}
                      onSelect={() =>
                        choose({ endpoint: target, model: typed })
                      }
                    />
                  ) : null}
                  <Command.Separator className="my-1 h-px bg-hairline" />
                  <HomeModelManage
                    label={t(
                      sections.length === 0
                        ? "home.tools.addEndpoint"
                        : "home.tools.manageEndpoints",
                    )}
                    onSelect={() => {
                      openChange(false);
                      onOpenServices();
                    }}
                  />
                </Command.List>
              </div>
            </div>
          </Command>
        </Popover.Content>
      </Popover.Portal>
    </Popover.Root>
  );
}
