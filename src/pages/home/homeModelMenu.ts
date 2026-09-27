import type { Provider } from "@/entities/provider";

/** The endpoint the tool uses now, whose models the picker lists. */
export interface ModelMenuEndpoint {
  /** The saved entry, for its logo; `null` for the sign-in or an outside address. */
  provider: Provider | null;
  /** The tool's own sign-in, shown with the tool's own mark. */
  signIn: boolean;
  name: string;
  /** Beside the name, e.g. where an outside address comes from. */
  detail: string | null;
  /** The name's tooltip: where an outside address is set. */
  title: string | null;
  /** The built-in list for an official service, else the endpoint's catalogue. */
  models: readonly string[];
  loading: boolean;
}

/** One choosable row: a model, or the tool's own default. */
export interface ModelMenuItem {
  id: string;
  /** `null` removes this product's model key so the tool decides. */
  model: string | null;
  label: string;
  note: string | null;
  checked: boolean;
}

export interface ModelMenuCopy {
  toolDefault: string;
  toolDefaultNote: string;
}

function unique(models: readonly (string | null)[]): string[] {
  return [...new Set(models.filter((model): model is string => !!model))];
}

/**
 * The model picker's rows (ADR-0055): the tool's default, then the model in
 * use and the endpoint's catalogue. Only the endpoint in use is listed;
 * changing endpoint is the API Endpoints page's job. The copy saved with the
 * entry is not listed: for the endpoint in use the tool's file is the truth,
 * and the saved copy may be a model chosen long ago.
 */
export function buildModelItems(
  endpoint: ModelMenuEndpoint,
  currentModel: string | null,
  copy: ModelMenuCopy,
): ModelMenuItem[] {
  const listed = unique([currentModel, ...endpoint.models]);
  return [
    {
      id: "\u001f",
      model: null,
      label: copy.toolDefault,
      note: copy.toolDefaultNote,
      checked: currentModel === null,
    },
    ...listed.map((model) => ({
      id: model,
      model,
      label: model,
      note: null,
      checked: model === currentModel,
    })),
  ];
}

/** The rows whose name or note contains the filter. */
export function filterModelItems(
  items: readonly ModelMenuItem[],
  query: string,
): ModelMenuItem[] {
  const needle = query.trim().toLocaleLowerCase();
  if (!needle) return [...items];
  return items.filter((item) =>
    [item.label, item.note].some((text) =>
      text?.toLocaleLowerCase().includes(needle),
    ),
  );
}
