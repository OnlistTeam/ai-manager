import { invokeNative } from "../client";
import {
  modelNameSchema,
  toolModelChoiceSchema,
  type ToolModelChoice,
} from "../schemas/modelChoice";
import type { ToolId } from "../schemas/tool";

/**
 * The model a tool runs and how hard it thinks, as chosen on Home
 * (ADR-0055). Only the tool's own keys are written; `null` removes this
 * product's key so the tool decides again.
 */
export const modelChoice = {
  get(tool: ToolId): Promise<ToolModelChoice> {
    return invokeNative("app_tool_model_choice", toolModelChoiceSchema, {
      tool,
    });
  },

  /**
   * Sets the model saved with one endpoint, and in the live file when that
   * endpoint is in use. With no endpoint named, only the connection in force
   * changes.
   */
  setModel(
    tool: ToolId,
    provider: string | null,
    model: string | null,
  ): Promise<ToolModelChoice> {
    return invokeNative("app_tool_model_set", toolModelChoiceSchema, {
      tool,
      provider,
      model: model === null ? null : modelNameSchema.parse(model),
    });
  },

  setEffort(tool: ToolId, effort: string | null): Promise<ToolModelChoice> {
    return invokeNative("app_tool_effort_set", toolModelChoiceSchema, {
      tool,
      effort,
    });
  },
};
