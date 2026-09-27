import {
  AlertTriangle,
  ChevronDown,
  LoaderCircle,
  Plus,
  Settings2,
} from "lucide-react";
import { useState } from "react";
import { useTranslation } from "react-i18next";
import type { Tool } from "@/entities/tool";
import {
  describeSource,
  externalPrecedenceCopy,
} from "@/features/provider-management";
import { Button } from "@/shared/ui/Button";
import { cn } from "@/shared/ui/cn";
import {
  OptionPicker,
  type OptionPickerAction,
} from "@/shared/ui/OptionPicker";
import { pillCopy } from "./homePillCopy";
import { useHomeModelMenu } from "./useHomeModelMenu";
import type { HomeToolConnection } from "./useHomeToolConnection";

export interface HomeServicePickerProps {
  tool: Tool;
  connection: HomeToolConnection;
  onOpenServices: () => void;
}

/** Every pill in the column shares one width so it lines up down the list. */
const PILL_CLASS = "h-7 w-64 shrink-0 rounded-full px-3 text-caption";

/** Unfiltered, an endpoint lists this many models; typing reaches the rest. */
const MODELS_PER_ENDPOINT = 8;

/**
 * The endpoint a tool uses and, where Home chooses it, its model (ADR-0055),
 * as a compact button. The list names each saved endpoint with its models
 * under it: picking an endpoint switches through the shared recoverable
 * flow, picking a model sets it (switching first when it sits under another
 * endpoint), and a typed model name can be used as typed. The list ends with
 * the way to the API Endpoints page (ADR-0053).
 */
export function HomeServicePicker({
  tool,
  connection,
  onOpenServices,
}: HomeServicePickerProps) {
  const { t } = useTranslation();
  const [open, setOpen] = useState(false);
  const canChooseModel = tool.capabilities.canChooseModel;
  const menu = useHomeModelMenu(tool, connection, connection.choice, open);
  const state = connection.connection;

  if (state.kind === "loading") {
    return (
      <span
        aria-hidden="true"
        className={cn(PILL_CLASS, "bg-layer-2 motion-safe:animate-pulse")}
      />
    );
  }

  const { label, detail } = pillCopy(state, connection, canChooseModel, t);

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
  const busy = switching !== null || connection.settingModel;
  const muted = state.kind === "notConnected";
  const additive = state.kind === "added";
  const external = state.kind === "external" ? state.connection : null;
  const empty = menu.groups.length === 0 && !additive;
  const action: OptionPickerAction =
    connection.choices.length === 0 && !additive && !external
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
  const select = (id: string) => {
    const choice = menu.choices.get(id);
    if (choice?.kind === "endpoint")
      connection.switchProvider(choice.providerId);
    if (choice?.kind === "model") {
      connection.chooseModel(choice.providerId, choice.model);
    }
  };
  // Where the model is not chosen here, the list stays the plain list of
  // endpoints it always was.
  const list = canChooseModel
    ? {
        groups: menu.groups,
        filterAbove: 0,
        groupLimit: MODELS_PER_ENDPOINT,
        moreLabel: (hidden: number) => t("home.model.more", { count: hidden }),
        freeEntry: {
          label: (text: string) => t("home.model.useTyped", { model: text }),
          onSelect: (text: string) =>
            connection.chooseModel(connection.inUseId, text),
        },
        contentClassName: "w-80",
      }
    : { options: menu.groups.map((group) => group.header) };

  return (
    <OptionPicker
      label={t(
        canChooseModel ? "home.model.pickerLabel" : "home.tools.pickerLabel",
        { tool: tool.name },
      )}
      {...list}
      note={note}
      empty={empty ? t("home.tools.noEndpoints") : undefined}
      actions={[action]}
      filterPlaceholder={t(
        canChooseModel ? "home.model.filter" : "home.tools.filter",
      )}
      noMatch={t(canChooseModel ? "home.model.noMatch" : "home.tools.noMatch")}
      onOpenChange={setOpen}
      onSelect={select}
    >
      <Button
        variant="secondary"
        size="xs"
        disabled={connection.switchDisabled || connection.settingModel}
        aria-label={t(
          canChooseModel ? "home.model.pickNamed" : "home.tools.pickNamed",
          {
            tool: tool.name,
            current: switching
              ? t("services.switch.switchingNamed", { name: switching })
              : [label, detail].filter(Boolean).join(", "),
          },
        )}
        className={cn(
          PILL_CLASS,
          "justify-between",
          external && "border-warning/40 bg-warning/10 hover:bg-warning/15",
        )}
      >
        {busy ? (
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
          <span className="max-w-[50%] shrink truncate text-content-muted">
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
