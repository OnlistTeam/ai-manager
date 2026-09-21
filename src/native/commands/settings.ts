import { z } from "zod";
import { invokeNative } from "../client";
import {
  productSettingsSchema,
  terminalAppIdSchema,
  type ProductSettings,
  type TerminalAppId,
} from "../schemas/settings";

const availableTerminalsSchema = z.array(terminalAppIdSchema).max(6);

export const settings = {
  get(): Promise<ProductSettings> {
    return invokeNative("app_settings_get", productSettingsSchema);
  },

  /**
   * Full replace (Decision 3). The backend returns the **read-back** result, so the
   * frontend never has to guess what actually ended up stored. Merging `Partial`
   * happens in `entities/settings`, in exactly that one place, not here.
   */
  save(next: ProductSettings): Promise<ProductSettings> {
    return invokeNative("app_settings_save", productSettingsSchema, {
      settings: next,
    });
  },

  /** The ones actually installed on this machine that can take over a terminal handoff. Empty = nothing to choose from. */
  availableTerminals(): Promise<TerminalAppId[]> {
    return invokeNative("app_terminals_list", availableTerminalsSchema);
  },
};
