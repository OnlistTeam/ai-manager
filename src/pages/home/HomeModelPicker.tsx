import * as Popover from "@radix-ui/react-popover";
import { Command } from "cmdk";
import { ChevronDown, LoaderCircle, Search } from "lucide-react";
import { useState } from "react";
import { useTranslation } from "react-i18next";
import type { Tool } from "@/entities/tool";
import { Button } from "@/shared/ui/Button";
import { cn } from "@/shared/ui/cn";
import {
  EndpointMark,
  HomeModelManage,
  HomeModelRows,
  HomeModelTyped,
} from "./HomeModelList";
import { filterModelItems, modelPillLabel } from "./homeModelMenu";
import { useHomeModelMenu } from "./useHomeModelMenu";
import type { HomeToolConnection } from "./useHomeToolConnection";

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
 * endpoint's mark. The list holds only the models of the endpoint in use;
 * a name typed into the filter can be used as typed. Changing endpoint is
 * left to the API Endpoints page, which the list's last row opens.
 */
export function HomeModelPicker({
  tool,
  connection,
  onOpenServices,
}: HomeModelPickerProps) {
  const { t } = useTranslation();
  const [open, setOpen] = useState(false);
  const [query, setQuery] = useState("");
  const { endpoint, items } = useHomeModelMenu(
    tool,
    connection,
    connection.choice,
    open,
  );

  const shown = filterModelItems(items, query);
  const typed = query.trim();
  const offerTyped =
    typed.length > 0 &&
    endpoint !== null &&
    !items.some((item) => item.model === typed);
  const message =
    endpoint === null
      ? t("home.tools.noEndpoints")
      : shown.length === 0 && !offerTyped
        ? t("home.model.noMatch")
        : null;

  const openChange = (next: boolean) => {
    setOpen(next);
    if (!next) setQuery("");
  };
  const choose = (model: string | null) => {
    openChange(false);
    connection.chooseModel(model);
  };

  const switching = connection.switchingName;
  const busy = switching !== null || connection.settingModel;
  const model = connection.model;
  const label = switching
    ? t("services.switch.switchingNamed", { name: switching })
    : model
      ? modelPillLabel(model)
      : t("home.model.toolDefault");
  const where = endpoint
    ? [endpoint.name, endpoint.detail].filter(Boolean).join(" · ")
    : t("home.tools.notConnected");
  // The pill shortens the id (ADR-0059); the tooltip keeps it whole.
  const title = [switching ? null : model, endpoint?.title ?? where]
    .filter(Boolean)
    .join("\n");

  return (
    <Popover.Root open={open} onOpenChange={openChange}>
      <Popover.Trigger asChild>
        <Button
          variant="secondary"
          size="xs"
          disabled={busy}
          title={title}
          aria-label={t("home.model.pickNamed", {
            tool: tool.name,
            current: `${label}, ${where}`,
          })}
          className={cn(MODEL_PILL_CLASS, "justify-between")}
        >
          {busy ? (
            <LoaderCircle
              className="h-3.5 w-3.5 shrink-0 motion-safe:animate-spin"
              aria-hidden="true"
            />
          ) : endpoint ? (
            <EndpointMark endpoint={endpoint} tool={tool.id} />
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
            "app-floating-menu z-[70] flex w-80 max-w-[calc(100vw-2rem)] flex-col overflow-hidden rounded-lg border outline-none animate-ds-overlay-in",
            "max-h-[min(24rem,var(--radix-popover-content-available-height))]",
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
            {message ? (
              <p className="px-4 pb-1 pt-2 text-caption text-content-muted">
                {message}
              </p>
            ) : null}
            <Command.List
              label={t("home.model.pickerLabel", { tool: tool.name })}
              className="scrollbar-subtle min-h-0 flex-1 overflow-y-auto overscroll-contain py-1"
            >
              {endpoint && shown.length > 0 ? (
                <HomeModelRows
                  endpoint={endpoint}
                  items={shown}
                  loadingLabel={t("home.model.loading")}
                  onChoose={choose}
                />
              ) : null}
              {offerTyped ? (
                <HomeModelTyped
                  label={t("home.model.useTyped", { model: typed })}
                  onSelect={() => choose(typed)}
                />
              ) : null}
              <Command.Separator className="my-1 h-px bg-hairline" />
              <HomeModelManage
                label={t(
                  endpoint === null
                    ? "home.tools.addEndpoint"
                    : "home.tools.manageEndpoints",
                )}
                onSelect={() => {
                  openChange(false);
                  onOpenServices();
                }}
              />
            </Command.List>
          </Command>
        </Popover.Content>
      </Popover.Portal>
    </Popover.Root>
  );
}
