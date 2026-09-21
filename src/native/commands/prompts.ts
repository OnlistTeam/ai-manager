import { invokeNative } from "../client";
import { extensionListSchema, type Extension } from "../schemas/extension";
import {
  promptDetailSchema,
  promptDraftSchema,
  type PromptDetail,
  type PromptDraft,
} from "../schemas/prompt";
import type { ToolId } from "../schemas/tool";

export const prompts = {
  get(tool: ToolId, prompt: string): Promise<PromptDetail> {
    return invokeNative("app_prompt_get", promptDetailSchema, { tool, prompt });
  },

  save(
    tool: ToolId,
    prompt: string | null,
    draft: PromptDraft,
  ): Promise<Extension[]> {
    return invokeNative("app_prompt_save", extensionListSchema, {
      tool,
      prompt,
      draft: promptDraftSchema.parse(draft),
    });
  },

  remove(tool: ToolId, prompt: string): Promise<Extension[]> {
    return invokeNative("app_prompt_remove", extensionListSchema, {
      tool,
      prompt,
    });
  },

  importCurrent(tool: ToolId): Promise<Extension[]> {
    return invokeNative("app_prompt_import_current", extensionListSchema, {
      tool,
    });
  },
};
