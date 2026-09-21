import { z } from "zod";
import { desktopAppIdSchema } from "./desktopApp";
import { toolIdSchema, type ToolId } from "./tool";

/** The three inner pages from Spec §36. Order matches Rust `ExtensionKind::ALL`. */
export const extensionKindSchema = z.enum(["skill", "mcp", "prompt"]);
export const extensionManagementSchema = z.enum(["managed", "detected"]);
export const localExtensionScopeStatusSchema = z.enum(["ready", "unavailable"]);
export const detectedSkillResourceActionSchema = z.enum(["browse", "edit"]);
export const detectedSkillResourceOpenOutcomeSchema = z.enum([
  "folderOpened",
  "editorOpened",
]);

/** CLI tools and GUI applications are deliberately disjoint extension targets. */
export const extensionScopeSchema = z.discriminatedUnion("kind", [
  z.object({ kind: z.literal("tool"), id: toolIdSchema }).strict(),
  z.object({ kind: z.literal("desktopApp"), id: desktopAppIdSchema }).strict(),
]);

/**
 * Aligned field for field with Rust's `domain::extension::Extension`.
 * The wire format deliberately has **no** field that could carry a config payload --
 * no `server`, no `content`, no path (Spec §36, "avoid showing raw JSON on first entry").
 */
export const extensionSchema = z.object({
  kind: extensionKindSchema,
  id: z.string(),
  scope: extensionScopeSchema,
  name: z.string(),
  description: z.string().nullable(),
  management: extensionManagementSchema,
  enabled: z.boolean(),
  /**
   * A prompt file is always `canDisable: false`: upstream only supports selecting a
   * single active prompt among several for the same tool, and turning off the last
   * one would clear that file. The UI drives its control from this, without knowing `kind`.
   */
  canDisable: z.boolean(),
});

export const extensionListSchema = z.array(extensionSchema);

const localExtensionKindSchema = z.enum(["skill", "mcp"]);

export const localExtensionScopeSchema = z.object({
  tool: toolIdSchema,
  kind: localExtensionKindSchema,
  status: localExtensionScopeStatusSchema,
});

export const localExtensionInventorySchema = z
  .object({
    items: z.array(extensionSchema).max(512),
    scopes: z.array(localExtensionScopeSchema).max(16),
    truncated: z.boolean(),
  })
  .superRefine((inventory, context) => {
    const scopeKeys = new Set<string>();
    for (const [index, scope] of inventory.scopes.entries()) {
      const key = `${scope.tool}:${scope.kind}`;
      if (scopeKeys.has(key)) {
        context.addIssue({
          code: z.ZodIssueCode.custom,
          path: ["scopes", index],
          message: "duplicate local extension scope",
        });
      }
      scopeKeys.add(key);
    }

    for (const [index, item] of inventory.items.entries()) {
      const scope = inventory.scopes.find(
        (candidate) =>
          item.scope.kind === "tool" &&
          candidate.tool === item.scope.id &&
          candidate.kind === item.kind,
      );
      if (item.management !== "detected" || scope?.status !== "ready") {
        context.addIssue({
          code: z.ZodIssueCode.custom,
          path: ["items", index],
          message:
            "local inventory items must belong to a ready detected scope",
        });
      }
    }
  });

export type ExtensionKind = z.infer<typeof extensionKindSchema>;
export type ExtensionManagement = z.infer<typeof extensionManagementSchema>;
export type DetectedSkillResourceAction = z.infer<
  typeof detectedSkillResourceActionSchema
>;
export type DetectedSkillResourceOpenOutcome = z.infer<
  typeof detectedSkillResourceOpenOutcomeSchema
>;
export type ExtensionScope = z.infer<typeof extensionScopeSchema>;
export type Extension = z.infer<typeof extensionSchema>;
export type LocalExtensionInventory = z.infer<
  typeof localExtensionInventorySchema
>;
export type LocalExtensionScope = z.infer<typeof localExtensionScopeSchema>;
export type LocalExtensionScopeStatus = z.infer<
  typeof localExtensionScopeStatusSchema
>;
export type { ToolId };

export function extensionScopeKey(scope: ExtensionScope): string {
  return `${scope.kind}:${scope.id}`;
}

export function toolExtensionScope(id: ToolId): ExtensionScope {
  return { kind: "tool", id };
}
