import type { Provider, ToolModelChoice } from "@/entities/provider";

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

/** How one model id reads on Home (ADR-0059). */
export interface ModelDisplay {
  /** The model's own name: the id after its last `/`, without `[1m]`. */
  name: string;
  /** The namespace right above the name (usually the lab), and `1M` for `[1m]`. */
  note: string | null;
  /** Claude Code's context marker on the id, `1M` or `2M`. */
  context: string | null;
}

const CONTEXT_MARKER = /\[([12])m\]$/i;

/**
 * Names a model the way Claude Code reads it, which drops everything up to the
 * last `/` and treats a trailing `[1m]` as the context size. One model then
 * looks the same whether an endpoint lists `gpt-5.6-sol`, `openai/gpt-5.6-sol`
 * or `anthropic/openai/gpt-5.6-sol[1m]`. Display only: the id written to the
 * tool's file stays as the endpoint listed it, with no more than the tool's
 * context marker added (`catalogModelIds`).
 */
export function describeModel(id: string): ModelDisplay {
  const trimmed = id.trim();
  const marker = CONTEXT_MARKER.exec(trimmed);
  const bare = marker ? trimmed.slice(0, marker.index) : trimmed;
  const slash = bare.lastIndexOf("/");
  const hasName = slash >= 0 && slash < bare.length - 1;
  const name = hasName ? bare.slice(slash + 1) : bare;
  const namespace =
    hasName && slash > 0 ? bare.slice(0, slash).split("/").pop() : "";
  const context = marker ? `${marker[1]}M` : null;
  const note = [namespace, context].filter(Boolean).join(" · ");
  return { name, note: note || null, context };
}

/** A catalogue entry, as far as the picker reads it. */
export interface CatalogModel {
  id: string;
  contextTokens?: number;
}

/**
 * The ids the picker offers for a catalogue. A tool that reads its context
 * window from a suffix on the name (Claude Code's `[1m]`) gets it on every
 * model the catalogue gives a window that long; without it the tool runs the
 * model with its short window. An id that already carries a marker, and a
 * model whose window the catalogue does not say, are offered as listed.
 */
export function catalogModelIds(
  models: readonly CatalogModel[],
  marker: ToolModelChoice["contextMarker"],
): string[] {
  return models.map(({ id, contextTokens }) =>
    marker &&
    contextTokens !== undefined &&
    contextTokens >= marker.minTokens &&
    !CONTEXT_MARKER.test(id.trim())
      ? `${id}${marker.suffix}`
      : id,
  );
}

/** The pill's text: the model's name, with the context size when it has one. */
export function modelPillLabel(id: string): string {
  const { name, context } = describeModel(id);
  return context ? `${name} · ${context}` : name;
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
    ...listed.map((model) => {
      const { name, note } = describeModel(model);
      return {
        id: model,
        model,
        label: name,
        note,
        checked: model === currentModel,
      };
    }),
  ];
}

/** The rows whose name, note or full model id contains the filter. */
export function filterModelItems(
  items: readonly ModelMenuItem[],
  query: string,
): ModelMenuItem[] {
  const needle = query.trim().toLocaleLowerCase();
  if (!needle) return [...items];
  return items.filter((item) =>
    [item.label, item.note, item.model].some((text) =>
      text?.toLocaleLowerCase().includes(needle),
    ),
  );
}
