import { useQueries } from "@tanstack/react-query";
import { useTranslation } from "react-i18next";
import {
  providerEditProfileQueryOptions,
  type Provider,
  type ToolModelChoice,
} from "@/entities/provider";
import type { Tool } from "@/entities/tool";
import {
  hostOf,
  modelCatalogQueryOptions,
  shortSourceCopy,
} from "@/features/provider-management";
import {
  buildModelMenu,
  type ModelMenu,
  type ModelMenuEndpoint,
} from "./homeModelMenu";
import { pinnedModel } from "./homeToolConnection";
import type { HomeToolConnection } from "./useHomeToolConnection";

/**
 * The model picker's list for one row (ADR-0055). Each saved endpoint's own
 * model and, for a custom endpoint, its model catalogue are read only while
 * the picker is open, through the same cached queries the endpoints page and
 * the model test use; an official endpoint lists the tool's built-in set.
 */
export function useHomeModelMenu(
  tool: Tool,
  connection: HomeToolConnection,
  choice: ToolModelChoice | undefined,
  open: boolean,
): ModelMenu {
  const { t } = useTranslation();
  const canChooseModel = tool.capabilities.canChooseModel;
  const reading = open && canChooseModel;
  const state = connection.connection;
  const external = state.kind === "external" ? state.connection : null;
  const catalogued = (provider: Provider) => provider.kind !== "official";

  const profiles = useQueries({
    queries: connection.choices.map((provider) => ({
      ...providerEditProfileQueryOptions(tool.id, provider.id),
      enabled: reading,
    })),
  });
  const catalogs = useQueries({
    queries: connection.choices.map((provider) => ({
      ...modelCatalogQueryOptions({ kind: "provider", provider }),
      enabled: reading && catalogued(provider),
    })),
  });
  const [effectiveCatalog] = useQueries({
    queries: [
      {
        ...modelCatalogQueryOptions({
          kind: "effective",
          tool: tool.id,
          name: tool.name,
        }),
        enabled: reading && external !== null,
      },
    ],
  });

  const textModels = (models: { id: string; kind: string }[] | undefined) =>
    (models ?? []).filter((model) => model.kind === "text").map((m) => m.id);
  const official = choice?.officialModels ?? [];

  const endpoints: ModelMenuEndpoint[] = [];
  if (external) {
    endpoints.push({
      providerId: null,
      name: hostOf(external.endpoint),
      detail: shortSourceCopy(external.endpointSource, t),
      inUse: true,
      savedModel: null,
      models: textModels(effectiveCatalog?.data?.models),
      loading: effectiveCatalog?.isFetching ?? false,
    });
  } else if (
    canChooseModel &&
    state.kind === "official" &&
    state.provider === null
  ) {
    // The tool's own sign-in has no saved entry, but its models can still be
    // chosen.
    endpoints.push({
      providerId: null,
      name: t("home.tools.official"),
      detail: null,
      inUse: true,
      savedModel: null,
      models: official,
      loading: false,
    });
  }
  connection.choices.forEach((provider, index) => {
    const catalog = catalogs[index];
    endpoints.push({
      providerId: provider.id,
      name: provider.name,
      detail: null,
      inUse: provider.id === connection.inUseId,
      savedModel: pinnedModel(profiles[index]?.data),
      models: catalogued(provider)
        ? textModels(catalog?.data?.models)
        : official,
      loading: catalogued(provider) && (catalog?.isFetching ?? false),
    });
  });

  return buildModelMenu(endpoints, choice?.model ?? null, canChooseModel, {
    toolDefault: t("home.model.toolDefault"),
    loading: t("home.model.loading"),
  });
}
