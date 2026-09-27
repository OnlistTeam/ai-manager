import type { TFunction } from "i18next";
import { hostOf, shortSourceCopy } from "@/features/provider-management";
import type { ToolConnection } from "./homeToolConnection";
import type { HomeToolConnection } from "./useHomeToolConnection";

const LABEL_KEYS: Record<
  Exclude<ToolConnection["kind"], "service" | "external" | "loading">,
  string
> = {
  unavailable: "home.tools.unavailable",
  official: "home.tools.official",
  added: "home.tools.added",
  notConnected: "home.tools.notConnected",
};

/**
 * What the pill says: the saved entry by the name its list uses (official
 * sign-in included), an address set outside this app by its host and where it
 * comes from, the endpoints page's own naming (ADR-0035); only a tool with no
 * entry falls back to a description. Beside a connection stands the model:
 * the one in the tool's file where Home chooses it (its default when the file
 * names none, ADR-0054), else the one the entry pins.
 */
export function pillCopy(
  state: Exclude<ToolConnection, { kind: "loading" }>,
  connection: HomeToolConnection,
  canChooseModel: boolean,
  t: TFunction,
): { label: string; detail: string | null } {
  const model =
    canChooseModel && connection.modelKnown
      ? (connection.model ?? t("home.model.toolDefault"))
      : connection.model;
  switch (state.kind) {
    case "service":
      return { label: state.provider.name, detail: model };
    case "external":
      return {
        label: hostOf(state.connection.endpoint),
        // Where it comes from matters more here than the model, which the
        // list shows.
        detail: shortSourceCopy(state.connection.endpointSource, t),
      };
    case "official":
      return {
        label: state.provider?.name ?? t(LABEL_KEYS.official),
        detail: state.provider || canChooseModel ? model : null,
      };
    case "added":
      return {
        label: t(LABEL_KEYS.added, { count: state.count }),
        detail: null,
      };
  }
  return { label: t(LABEL_KEYS[state.kind]), detail: null };
}
