import {
  AlertTriangle,
  ChevronDown,
  LoaderCircle,
  Plus,
  Settings2,
} from "lucide-react";
import type { TFunction } from "i18next";
import { useTranslation } from "react-i18next";
import type { Tool } from "@/entities/tool";
import {
  describeSource,
  externalPrecedenceCopy,
  hostOf,
  shortSourceCopy,
} from "@/features/provider-management";
import { Button } from "@/shared/ui/Button";
import { cn } from "@/shared/ui/cn";
import {
  OptionPicker,
  type OptionPickerAction,
  type OptionPickerOption,
} from "@/shared/ui/OptionPicker";
import type { ToolConnection } from "./homeToolConnection";
import type { HomeToolConnection } from "./useHomeToolConnection";

export interface HomeServicePickerProps {
  tool: Tool;
  connection: HomeToolConnection;
  onOpenServices: () => void;
}

const LABEL_KEYS: Record<
  Exclude<ToolConnection["kind"], "service" | "external" | "loading">,
  string
> = {
  unavailable: "home.tools.unavailable",
  official: "home.tools.official",
  added: "home.tools.added",
  notConnected: "home.tools.notConnected",
};

/** Every pill shares one width so the column lines up down the list. */
const PILL_CLASS = "h-7 w-56 shrink-0 rounded-full px-3 text-caption";

/** The external entry's id in the list; it is never selectable. */
const EXTERNAL_OPTION_ID = "external";

/**
 * What the pill says: the saved entry by the name its list uses (official
 * sign-in included), an address set outside this app by its host and where it
 * comes from, the endpoints page's own naming (ADR-0035); only a tool with no
 * entry falls back to a description.
 */
function pillCopy(
  state: Exclude<ToolConnection, { kind: "loading" }>,
  model: string | null,
  t: TFunction,
): { label: string; detail: string | null } {
  switch (state.kind) {
    case "service":
      return { label: state.provider.name, detail: model };
    case "external":
      return {
        label: hostOf(state.connection.endpoint),
        detail: shortSourceCopy(state.connection.endpointSource, t),
      };
    case "official":
      if (state.provider) return { label: state.provider.name, detail: model };
      break;
    case "added":
      return {
        label: t(LABEL_KEYS.added, { count: state.count }),
        detail: null,
      };
  }
  return { label: t(LABEL_KEYS[state.kind]), detail: null };
}

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

  const { label, detail } = pillCopy(state, connection.model, t);

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
  const external = state.kind === "external" ? state.connection : null;
  const empty = connection.choices.length === 0 && !additive && !external;
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
  // What the tool really uses leads the list even when it is none of the
  // saved entries; it can only be changed where it was set.
  const options: OptionPickerOption[] = [
    ...(external
      ? [
          {
            id: EXTERNAL_OPTION_ID,
            label,
            detail,
            checked: true,
            disabled: true,
          },
        ]
      : []),
    ...connection.choices.map((provider) => ({
      id: provider.id,
      label: provider.name,
      detail: provider.id === connection.inUseId ? connection.model : null,
      checked: provider.id === connection.inUseId,
    })),
  ];
  const note = external ? (
    <>
      <span className="block">
        {describeSource(external.endpointSource, t)}
      </span>
      <span className="block">
        {externalPrecedenceCopy(external, tool.name, t)}
      </span>
    </>
  ) : additive ? (
    t("home.tools.modelInTool", { tool: tool.name })
  ) : undefined;

  return (
    <OptionPicker
      label={t("home.tools.pickerLabel", { tool: tool.name })}
      options={options}
      note={note}
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
            : [label, detail].filter(Boolean).join(", "),
        })}
        className={cn(
          PILL_CLASS,
          "justify-between",
          external && "border-warning/40 bg-warning/10 hover:bg-warning/15",
        )}
      >
        {switching ? (
          <LoaderCircle
            className="h-3.5 w-3.5 shrink-0 motion-safe:animate-spin"
            aria-hidden="true"
          />
        ) : external ? (
          <AlertTriangle
            className="h-3.5 w-3.5 shrink-0 text-warning"
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
        {detail && !switching ? (
          <span className="max-w-[45%] shrink truncate text-content-muted">
            {detail}
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
