import { useQuery } from "@tanstack/react-query";
import { useTranslation } from "react-i18next";
import type { ToolModelChoice } from "@/entities/provider";
import type { Tool } from "@/entities/tool";
import {
  describeSource,
  hostOf,
  modelCatalogQueryOptions,
  shortSourceCopy,
} from "@/features/provider-management";
import {
  buildModelItems,
  type ModelMenuEndpoint,
  type ModelMenuItem,
} from "./homeModelMenu";
import type { HomeToolConnection } from "./useHomeToolConnection";

export interface HomeModelMenu {
  /** The endpoint the tool uses now, if any: what the button shows. */
  endpoint: ModelMenuEndpoint | null;
  items: ModelMenuItem[];
}

/**
 * The model picker's rows for one row (ADR-0055): the models of the endpoint
 * in use. A custom endpoint's catalogue, or the one behind an outside
 * address, is read only while the picker is open, through the same cached
 * query the endpoints page and the model test use; an official endpoint
 * lists the tool's built-in set.
 */
export function useHomeModelMenu(
  tool: Tool,
  connection: HomeToolConnection,
  choice: ToolModelChoice | undefined,
  open: boolean,
): HomeModelMenu {
  const { t } = useTranslation();
  const state = connection.connection;
  const external = state.kind === "external" ? state.connection : null;
  const provider =
    state.kind === "service" || state.kind === "official"
      ? state.provider
      : null;
  const subject = external
    ? { kind: "effective" as const, tool: tool.id, name: tool.name }
    : provider && provider.kind !== "official"
      ? { kind: "provider" as const, provider }
      : null;
  const catalog = useQuery({
    ...modelCatalogQueryOptions(subject),
    enabled: open && subject !== null,
  });

  const shared = {
    models: subject
      ? (catalog.data?.models ?? [])
          .filter((model) => model.kind === "text")
          .map((model) => model.id)
      : (choice?.officialModels ?? []),
    loading: subject !== null && catalog.isFetching,
  };
  let endpoint: ModelMenuEndpoint | null = null;
  if (external) {
    endpoint = {
      ...shared,
      provider: null,
      signIn: false,
      name: hostOf(external.endpoint),
      detail: shortSourceCopy(external.endpointSource, t),
      title: describeSource(external.endpointSource, t),
    };
  } else if (provider) {
    endpoint = {
      ...shared,
      provider,
      signIn: false,
      name: provider.name,
      detail: null,
      title: null,
    };
  } else if (state.kind === "official") {
    // The tool's own sign-in has no saved entry, but its models can still be
    // chosen.
    endpoint = {
      ...shared,
      provider: null,
      signIn: true,
      name: t("home.tools.official"),
      detail: null,
      title: null,
    };
  }

  return {
    endpoint,
    items: endpoint
      ? buildModelItems(endpoint, choice?.model ?? null, {
          toolDefault: t("home.model.toolDefault"),
          toolDefaultNote: t("home.model.toolDefaultNote", { tool: tool.name }),
        })
      : [],
  };
}
