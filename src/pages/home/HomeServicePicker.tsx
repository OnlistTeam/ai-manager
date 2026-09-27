import { ChevronDown, LoaderCircle, Plus, Settings2 } from "lucide-react";
import { useTranslation } from "react-i18next";
import type { Tool } from "@/entities/tool";
import { Button } from "@/shared/ui/Button";
import { cn } from "@/shared/ui/cn";
import {
  OptionPicker,
  type OptionPickerAction,
} from "@/shared/ui/OptionPicker";
import type { ToolConnection } from "./homeToolConnection";
import type { HomeToolConnection } from "./useHomeToolConnection";

export interface HomeServicePickerProps {
  tool: Tool;
  connection: HomeToolConnection;
  onOpenServices: () => void;
}

const LABEL_KEYS: Record<
  Exclude<ToolConnection["kind"], "service" | "loading">,
  string
> = {
  unavailable: "home.tools.unavailable",
  official: "home.tools.official",
  added: "home.tools.added",
  notConnected: "home.tools.notConnected",
};

/** Every pill shares one width so the column lines up down the list. */
const PILL_CLASS = "h-7 w-56 shrink-0 rounded-full px-3 text-caption";

/**
 * The endpoint a tool uses, as a compact button that opens the tool's saved
 * endpoints. Picking one switches through the shared recoverable flow; the
 * list ends with the way to the API Endpoints page, where endpoints are
 * added, checked and ordered (ADR-0053).
 */
export function HomeServicePicker({
  tool,
  connection,
  onOpenServices,
}: HomeServicePickerProps) {
  const { t } = useTranslation();
  const state = connection.connection;

  if (state.kind === "loading") {
    return (
      <span
        aria-hidden="true"
        className={cn(PILL_CLASS, "bg-layer-2 motion-safe:animate-pulse")}
      />
    );
  }

  // A saved entry keeps the name the list below shows for it, official
  // sign-in included; only a tool with no entry falls back to a description.
  const label =
    state.kind === "service"
      ? state.provider.name
      : state.kind === "official" && state.provider
        ? state.provider.name
        : t(LABEL_KEYS[state.kind], {
            count: state.kind === "added" ? state.count : undefined,
          });

  // Nothing to choose from until the list can be read again; the pill only
  // says so.
  if (state.kind === "unavailable") {
    return (
      <span
        title={label}
        className={cn(
          PILL_CLASS,
          "flex items-center truncate border border-dashed border-hairline text-content-muted",
        )}
      >
        {label}
      </span>
    );
  }

  const switching = connection.switchingName;
  const muted = state.kind === "notConnected";
  const additive = state.kind === "added";
  const empty = connection.choices.length === 0 && !additive;
  const action: OptionPickerAction = empty
    ? {
        id: "add",
        label: t("home.tools.addEndpoint"),
        icon: Plus,
        onSelect: onOpenServices,
      }
    : {
        id: "manage",
        label: t("home.tools.manageEndpoints"),
        icon: Settings2,
        onSelect: onOpenServices,
      };

  return (
    <OptionPicker
      label={t("home.tools.pickerLabel", { tool: tool.name })}
      options={connection.choices.map((provider) => ({
        id: provider.id,
        label: provider.name,
        detail: provider.active ? connection.model : null,
        checked: provider.active,
      }))}
      note={
        additive ? t("home.tools.modelInTool", { tool: tool.name }) : undefined
      }
      empty={empty ? t("home.tools.noEndpoints") : undefined}
      actions={[action]}
      filterPlaceholder={t("home.tools.filter")}
      noMatch={t("home.tools.noMatch")}
      onSelect={connection.switchProvider}
    >
      <Button
        variant="secondary"
        size="xs"
        disabled={connection.switchDisabled}
        aria-label={t("home.tools.pickNamed", {
          tool: tool.name,
          current: switching
            ? t("services.switch.switchingNamed", { name: switching })
            : label,
        })}
        className={cn(PILL_CLASS, "justify-between")}
      >
        {switching ? (
          <LoaderCircle
            className="h-3.5 w-3.5 shrink-0 motion-safe:animate-spin"
            aria-hidden="true"
          />
        ) : null}
        <span
          className={cn(
            "min-w-0 flex-1 truncate text-left",
            muted ? "text-content-muted" : "text-content",
          )}
        >
          {switching ?? label}
        </span>
        {connection.model && !switching ? (
          <span className="max-w-[45%] shrink truncate text-content-muted">
            {connection.model}
          </span>
        ) : null}
        <ChevronDown
          className="h-3.5 w-3.5 shrink-0 text-content-muted"
          aria-hidden="true"
        />
      </Button>
    </OptionPicker>
  );
}
