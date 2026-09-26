import { ChevronRight, LoaderCircle } from "lucide-react";
import { useId } from "react";
import { useTranslation } from "react-i18next";
import type { Tool, ToolId } from "@/entities/tool";
import { Button } from "@/shared/ui/Button";
import { ListGroupRow } from "@/shared/ui/ListGroup";
import { ToolGlyph } from "@/shared/ui/ToolArtwork";
import type { ToolConnection } from "./homeToolConnection";
import { HomeToolRowNotice } from "./HomeToolRowNotice";
import { useHomeToolConnection } from "./useHomeToolConnection";

export interface HomeToolRowUpdate {
  disabled: boolean;
  running: boolean;
  onSelect: () => void;
}

export interface HomeToolRowProps {
  tool: Tool;
  /** Present only while the tool has an update. */
  update: HomeToolRowUpdate | null;
  /** Offered on the switch toast; undefined when the tool cannot launch. */
  onOpenTool?: () => void;
  onOpenServices: (toolId: ToolId) => void;
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

/** Controls that must stay clickable above the row's stretched open target. */
const ABOVE_ROW_TARGET = "relative z-10";

/**
 * One installed tool and the endpoint it uses: name, connection, the pinned
 * model, then the few things to do about it. The whole row opens the tool's
 * API Endpoints; the chevron is that same target for keyboard users.
 */
export function HomeToolRow({
  tool,
  update,
  onOpenTool,
  onOpenServices,
}: HomeToolRowProps) {
  const { t } = useTranslation();
  const headingId = useId();
  const connection = useHomeToolConnection(tool, onOpenTool);
  const state = connection.connection;
  const label =
    state.kind === "service"
      ? state.provider.name
      : state.kind === "loading"
        ? null
        : t(LABEL_KEYS[state.kind], {
            count: state.kind === "added" ? state.count : undefined,
          });
  const detail = connection.switchingName
    ? t("services.switch.switchingNamed", { name: connection.switchingName })
    : state.kind === "added"
      ? t("home.tools.modelInTool", { tool: tool.name })
      : connection.model;
  const openServices = () => onOpenServices(tool.id);

  return (
    <ListGroupRow
      interactive
      role="article"
      aria-labelledby={headingId}
      aria-busy={
        state.kind === "loading" || connection.switchingName !== null
          ? true
          : undefined
      }
      className="flex flex-col gap-2 px-4 py-2.5"
    >
      <div className="relative grid min-w-0 grid-cols-[minmax(0,1fr)_auto] items-center gap-x-4 gap-y-1 lg:grid-cols-[minmax(0,11rem)_minmax(0,1fr)_auto]">
        <div className="flex min-w-0 items-center gap-3">
          <span className="flex h-8 w-8 shrink-0 items-center justify-center rounded-lg border border-hairline bg-layer-1">
            <ToolGlyph toolId={tool.id} className="h-5 w-5" />
          </span>
          <h3
            id={headingId}
            className="truncate text-body font-medium text-content"
          >
            {tool.name}
          </h3>
        </div>

        <div className="col-span-2 row-start-2 flex min-w-0 items-baseline gap-2 pl-11 lg:col-span-1 lg:col-start-2 lg:row-start-1 lg:pl-0">
          {label === null ? (
            <span
              aria-hidden="true"
              className="h-3 w-24 self-center rounded bg-layer-2 motion-safe:animate-pulse"
            />
          ) : (
            <span
              data-connection={state.kind}
              className={
                state.kind === "notConnected" || state.kind === "unavailable"
                  ? "max-w-full shrink-0 truncate text-body text-content-muted"
                  : "max-w-full shrink-0 truncate text-body text-content"
              }
            >
              {label}
            </span>
          )}
          {detail ? (
            <span
              role={connection.switchingName ? "status" : undefined}
              className="flex min-w-0 items-center gap-1 truncate text-caption text-content-muted"
            >
              {connection.switchingName ? (
                <LoaderCircle
                  className="h-3.5 w-3.5 shrink-0 motion-safe:animate-spin"
                  aria-hidden="true"
                />
              ) : null}
              <span className="truncate">{detail}</span>
            </span>
          ) : null}
        </div>

        <div className="col-start-2 row-start-1 flex shrink-0 items-center justify-end gap-2 lg:col-start-3">
          {connection.choices.length > 0 ? (
            <select
              aria-label={t("home.tools.switchNamed", { tool: tool.name })}
              value=""
              disabled={connection.switchDisabled}
              onChange={(event) => {
                if (event.target.value) {
                  connection.switchProvider(event.target.value);
                }
              }}
              className={`${ABOVE_ROW_TARGET} h-8 w-32 truncate text-caption disabled:opacity-50`}
            >
              <option value="" disabled>
                {t("home.tools.switchTo")}
              </option>
              {connection.choices.map((provider) => (
                <option key={provider.id} value={provider.id}>
                  {provider.name}
                </option>
              ))}
            </select>
          ) : state.kind === "notConnected" ? (
            <Button
              variant="secondary"
              size="xs"
              className={ABOVE_ROW_TARGET}
              aria-label={t("home.tools.connectNamed", { tool: tool.name })}
              onClick={openServices}
            >
              {t("home.tools.connect")}
            </Button>
          ) : null}
          {update ? (
            <Button
              variant="secondary"
              size="xs"
              className={ABOVE_ROW_TARGET}
              loading={update.running}
              disabled={update.disabled}
              aria-label={t("home.tools.updateNamed", { tool: tool.name })}
              onClick={update.onSelect}
            >
              {t("home.tools.update")}
            </Button>
          ) : null}
          <Button
            variant="ghost"
            size="xs"
            className="w-7 px-0 after:absolute after:inset-0 after:content-['']"
            aria-label={t("home.tools.openNamed", { tool: tool.name })}
            onClick={openServices}
          >
            <ChevronRight className="h-4 w-4" aria-hidden="true" />
          </Button>
        </div>
      </div>

      <HomeToolRowNotice
        connection={connection}
        onOpenServices={openServices}
      />
    </ListGroupRow>
  );
}
