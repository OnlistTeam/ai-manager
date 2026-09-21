import { invokeNative } from "../client";
import {
  importOutcomeSchema,
  importPreviewSchema,
  type ImportOutcome,
  type ImportPreview,
} from "../schemas/import";

export const importExisting = {
  preview(): Promise<ImportPreview> {
    return invokeNative("app_import_preview", importPreviewSchema);
  },

  run(): Promise<ImportOutcome> {
    return invokeNative("app_import_run", importOutcomeSchema);
  },
};
