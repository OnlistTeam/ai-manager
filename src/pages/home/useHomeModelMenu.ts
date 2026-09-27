import { useQueries } from "@tanstack/react-query";
import { useTranslation } from "react-i18next";
import {
  providerEditProfileQueryOptions,
  type Provider,
  type ToolModelChoice,
} from "@/entities/provider";
import type { Tool } from "@/entities/tool";
import {
  describeSource,
  externalPrecedenceCopy,
  hostOf,
  modelCatalogQueryOptions,
  shortSourceCopy,
} from "@/features/provider-management";
import {
  buildModelSections,
  type ModelMenuEndpoint,
  type ModelMenuSection,
} from "./homeModelMenu";
import { pinnedModel } from "./homeToolConnection";
import type { HomeToolConnection } from "./useHomeToolConnection";

export interface HomeModelMenu {
  sections: ModelMenuSection[];
  /** The endpoint the tool uses now, if any: what the button shows. */
  inUse: ModelMenuEndpoint | null;
}

/**
 * The model picker's sections for one row (ADR-0055). Each saved endpoint's
 * own model and, for a custom endpoint, its model catalogue are read only
 * while the picker is open, through the same cached queries the endpoints
 * page and the model test use; an official endpoint lists the tool's
 * built-in set.
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
  const catalogued = (provider: Provider) => provider.kind !== "official";

  const profiles = useQueries({
    queries: connection.choices.map((provider) => ({
      ...providerEditProfileQueryOptions(tool.id, provider.id),
      enabled: open,
    })),
  });
  const catalogs = useQueries({
    queries: connection.choices.map((provider) => ({
      ...modelCatalogQueryOptions({ kind: "provider", provider }),
      enabled: open && catalogued(provider),
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
        enabled: open && external !== null,
      },
    ],
  });

  const textModels = (models: { id: string; kind: string }[] | undefined) =>
    (models ?? []).filter((model) => model.kind === "text").map((m) => m.id);
  const official = choice?.officialModels ?? [];
  const unsaved = {
    providerId: null,
    provider: null,
    inUse: true,
    savedModel: null,
  };

  const endpoints: ModelMenuEndpoint[] = [];
  if (external) {
    endpoints.push({
      ...unsaved,
      key: "outside",
      signIn: false,
      name: hostOf(external.endpoint),
      detail: shortSourceCopy(external.endpointSource, t),
      title: [
        describeSource(external.endpointSource, t),
        externalPrecedenceCopy(external, tool.name, t),
      ].join("\n"),
      models: textModels(effectiveCatalog?.data?.models),
      loading: effectiveCatalog?.isFetching ?? false,
    });
  } else if (state.kind === "official" && state.provider === null) {
    // The tool's own sign-in has no saved entry, but its models can still be
    // chosen.
    endpoints.push({
      ...unsaved,
      key: "signIn",
      signIn: true,
      name: t("home.tools.official"),
      detail: null,
      title: null,
      models: official,
      loading: false,
    });
  }
  connection.choices.forEach((provider, index) => {
    const catalog = catalogs[index];
    endpoints.push({
      key: provider.id,
      providerId: provider.id,
      provider,
      signIn: false,
      name: provider.name,
      detail: null,
      title: null,
      inUse: provider.id === connection.inUseId,
      savedModel: pinnedModel(profiles[index]?.data),
      models: catalogued(provider)
        ? textModels(catalog?.data?.models)
        : official,
      loading: catalogued(provider) && (catalog?.isFetching ?? false),
    });
  });

  const sections = buildModelSections(endpoints, choice?.model ?? null, {
    toolDefault: t("home.model.toolDefault"),
    toolDefaultNote: t("home.model.toolDefaultNote", { tool: tool.name }),
  });
  return {
    sections,
    inUse: endpoints.find((endpoint) => endpoint.inUse) ?? null,
  };
}
