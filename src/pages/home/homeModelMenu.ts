import type { OptionPickerGroup } from "@/shared/ui/OptionPicker";

/** What picking an entry in a row's model picker does (ADR-0054). */
export type ModelMenuChoice =
  | { kind: "endpoint"; providerId: string }
  | { kind: "model"; providerId: string | null; model: string | null };

/** One endpoint of the tool, with the models listed under it. */
export interface ModelMenuEndpoint {
  /**
   * The saved entry. `null` for what the tool uses without one: its own
   * sign-in, or an address set outside this app. Neither can be switched to,
   * but a model can still be set for it.
   */
  providerId: string | null;
  name: string;
  /** Beside the name, e.g. where an outside address comes from. */
  detail: string | null;
  inUse: boolean;
  /** The model saved with the endpoint: the one last used with it. */
  savedModel: string | null;
  /** The built-in list for an official service, else the endpoint's catalogue. */
  models: readonly string[];
  loading: boolean;
}

export interface ModelMenuCopy {
  toolDefault: string;
  loading: string;
}

export interface ModelMenu {
  groups: OptionPickerGroup[];
  choices: ReadonlyMap<string, ModelMenuChoice>;
}

function unique(models: readonly (string | null)[]): string[] {
  return [...new Set(models.filter((model): model is string => !!model))];
}

/**
 * The model picker's list: each endpoint as a heading, the one in use first,
 * with its models under it. Only under the endpoint in use can the model be
 * reset to the tool's own default; any other endpoint is switched to with
 * the model saved for it, or with the model picked under it.
 */
export function buildModelMenu(
  endpoints: readonly ModelMenuEndpoint[],
  currentModel: string | null,
  canChooseModel: boolean,
  copy: ModelMenuCopy,
): ModelMenu {
  const choices = new Map<string, ModelMenuChoice>();
  const groups = endpoints.map((endpoint, index): OptionPickerGroup => {
    const headerId = `endpoint:${index}`;
    if (endpoint.providerId !== null) {
      choices.set(headerId, {
        kind: "endpoint",
        providerId: endpoint.providerId,
      });
    }
    const header = {
      id: headerId,
      label: endpoint.name,
      detail: endpoint.inUse
        ? endpoint.detail
        : (endpoint.savedModel ?? endpoint.detail),
      checked: endpoint.inUse,
      disabled: endpoint.providerId === null,
    };
    if (!canChooseModel) return { id: headerId, header, options: [] };

    const listed = unique([
      endpoint.inUse ? currentModel : null,
      endpoint.savedModel,
      ...endpoint.models,
    ]);
    const options = listed.map((model, position) => {
      const id = `model:${index}:${position}`;
      choices.set(id, {
        kind: "model",
        providerId: endpoint.providerId,
        model,
      });
      return {
        id,
        label: model,
        checked: endpoint.inUse && model === currentModel,
      };
    });
    if (endpoint.inUse) {
      const id = `default:${index}`;
      choices.set(id, {
        kind: "model",
        providerId: endpoint.providerId,
        model: null,
      });
      options.unshift({
        id,
        label: copy.toolDefault,
        checked: currentModel === null,
      });
    }
    return {
      id: headerId,
      header,
      options,
      note: endpoint.loading && listed.length === 0 ? copy.loading : null,
    };
  });
  return { groups, choices };
}
