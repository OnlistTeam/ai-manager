import {
  AlertTriangle,
  ChevronDown,
  LoaderCircle,
  Plus,
  Settings2,
} from "lucide-react";
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
import { MODEL_PILL_CLASS as PILL_CLASS } from "./HomeModelPicker";
import { pillCopy } from "./homePillCopy";
import type { HomeToolConnection } from "./useHomeToolConnection";

export interface HomeServicePickerProps {
  tool: Tool;
  connection: HomeToolConnection;
  onOpenServices: () => void;
}

/**
 * The endpoint a tool uses, for a tool whose model is not chosen on Home
 * (ADR-0055 decision 7), and the placeholder any row shows while its
 * endpoints load or cannot be read. The list names each saved endpoint;
 * picking one switches through the shared recoverable flow. It ends with the
 * way to the API Endpoints page (ADR-0053).
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

  const { label, detail } = pillCopy(state, connection, t);

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
  const busy = switching !== null;
  const muted = state.kind === "notConnected";
  const additive = state.kind === "added";
  const external = state.kind === "external" ? state.connection : null;
  // Only an outside address the tool reads ahead of a switch is a problem;
  // any other is just where the tool is connected now.
  const outranked = external?.outranksSwitch ?? false;
  const options: OptionPickerOption[] = [
    ...(external
      ? [
          {
            id: "outside",
            label: hostOf(external.endpoint),
            detail: shortSourceCopy(external.endpointSource, t),
            checked: true,
            disabled: true,
          },
        ]
      : []),
    ...connection.choices.map((provider) => ({
      id: provider.id,
      label: provider.name,
      checked: !external && provider.id === connection.inUseId,
    })),
  ];
  const empty = options.length === 0 && !additive;
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
  const note = additive
    ? t("home.tools.modelInTool", { tool: tool.name })
    : undefined;
  return (
    <OptionPicker
      label={t("home.tools.pickerLabel", { tool: tool.name })}
      options={options}
      note={note}
      empty={empty ? t("home.tools.noEndpoints") : undefined}
      actions={[action]}
      filterPlaceholder={t("home.tools.filter")}
      noMatch={t("home.tools.noMatch")}
      onSelect={(id) => {
        if (id !== "outside") connection.switchProvider(id);
      }}
    >
      <Button
        variant="secondary"
        size="xs"
        disabled={connection.switchDisabled}
        title={
          external
            ? [
                describeSource(external.endpointSource, t),
                externalPrecedenceCopy(external, tool.name, t),
              ].join("\n")
            : undefined
        }
        aria-label={t("home.tools.pickNamed", {
          tool: tool.name,
          current: switching
            ? t("services.switch.switchingNamed", { name: switching })
            : [label, detail].filter(Boolean).join(", "),
        })}
        className={cn(
          PILL_CLASS,
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
