import type { Provider } from "@/entities/provider";

/** One endpoint of the tool, with the models listed under it. */
export interface ModelMenuEndpoint {
  /** Unique within the row: the saved entry's id, or `outside` / `signIn`. */
  key: string;
  /**
   * The saved entry. `null` for what the tool uses without one: its own
   * sign-in, or an address set outside this app. Neither can be switched to,
   * but a model can still be set for it.
   */
  providerId: string | null;
  /** The saved entry, for its logo. */
  provider: Provider | null;
  /** The tool's own sign-in, shown with the tool's own mark. */
  signIn: boolean;
  name: string;
  /** Beside the name, e.g. where an outside address comes from. */
  detail: string | null;
  /** The name's tooltip, e.g. the file and line an outside address is set in. */
  title: string | null;
  inUse: boolean;
  /** The model saved with the endpoint: the one last used with it. */
  savedModel: string | null;
  /** The built-in list for an official service, else the endpoint's catalogue. */
  models: readonly string[];
  loading: boolean;
}

/** One choosable row: a model under an endpoint, or that endpoint's default. */
export interface ModelMenuItem {
  id: string;
  endpoint: ModelMenuEndpoint;
  /** `null` removes this product's model key so the tool decides. */
  model: string | null;
  label: string;
  note: string | null;
  checked: boolean;
}

export interface ModelMenuSection {
  endpoint: ModelMenuEndpoint;
  items: ModelMenuItem[];
}

export interface ModelMenuCopy {
  toolDefault: string;
  toolDefaultNote: string;
}

/** Everything, the starred models, or one endpoint's models. */
export type ModelMenuView = "all" | "favorites" | { endpoint: string };

function unique(models: readonly (string | null)[]): string[] {
  return [...new Set(models.filter((model): model is string => !!model))];
}

/**
 * The model picker's sections (ADR-0055): the endpoint in use first, then the
 * rest in the user's order. Each starts with the tool's default, then the
 * model in use, the endpoint's saved model and its catalogue. Picking under
 * another endpoint switches to it first.
 */
export function buildModelSections(
  endpoints: readonly ModelMenuEndpoint[],
  currentModel: string | null,
  copy: ModelMenuCopy,
): ModelMenuSection[] {
  const ordered = [
    ...endpoints.filter((endpoint) => endpoint.inUse),
    ...endpoints.filter((endpoint) => !endpoint.inUse),
  ];
  return ordered.map((endpoint, index) => {
    const listed = unique([
      endpoint.inUse ? currentModel : null,
      endpoint.savedModel,
      ...endpoint.models,
    ]);
    const items: ModelMenuItem[] = [
      {
        id: `${endpoint.key}\u001f`,
        endpoint,
        model: null,
        label: copy.toolDefault,
        // Said once, at the top; the other sections' defaults are the same.
        note: index === 0 ? copy.toolDefaultNote : null,
        checked: endpoint.inUse && currentModel === null,
      },
      ...listed.map((model) => ({
        id: `${endpoint.key}\u001f${model}`,
        endpoint,
        model,
        label: model,
        note: null,
        checked: endpoint.inUse && model === currentModel,
      })),
    ];
    return { endpoint, items };
  });
}

/**
 * What the list shows for a view and a filter. A filter keeps the rows whose
 * name or note contains it, in every endpoint the view covers; an endpoint
 * left with no row is dropped.
 */
export function viewModelSections(
  sections: readonly ModelMenuSection[],
  view: ModelMenuView,
  query: string,
  isFavorite: (item: ModelMenuItem) => boolean,
): ModelMenuSection[] {
  const needle = query.trim().toLocaleLowerCase();
  return sections
    .filter(
      (section) =>
        typeof view === "string" || section.endpoint.key === view.endpoint,
    )
    .map((section) => ({
      endpoint: section.endpoint,
      items: section.items.filter(
        (item) =>
          (view !== "favorites" || isFavorite(item)) &&
          (!needle ||
            [item.label, item.note].some((text) =>
              text?.toLocaleLowerCase().includes(needle),
            )),
      ),
    }))
    .filter((section) => section.items.length > 0);
}
