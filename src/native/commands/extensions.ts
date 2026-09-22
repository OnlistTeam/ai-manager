import { z } from "zod";
import { invokeNative } from "../client";
import {
  detectedSkillResourceActionSchema,
  detectedSkillResourceOpenOutcomeSchema,
  extensionListSchema,
  extensionLocationActionSchema,
  extensionLocationSchema,
  localExtensionInventorySchema,
  type Extension,
  type DetectedSkillResourceAction,
  type DetectedSkillResourceOpenOutcome,
  type ExtensionKind,
  type ExtensionLocation,
  type ExtensionLocationAction,
  type ExtensionScope,
  type LocalExtensionInventory,
  type ToolId,
} from "../schemas/extension";

/**
 * A skill's id is its directory name. The backend validates this again
 * (`require_valid_directory`), but an id containing a path separator is a local bug that
 * shouldn't need a round trip through IPC to surface.
 */
const skillIdSchema = z
  .string()
  .min(1)
  .max(255)
  .refine(
    (value) =>
      !value.includes("/") && !value.includes("\\") && !value.includes("\0"),
  );

export const extensions = {
  revealLocation(scope: ExtensionScope, kind: ExtensionKind): Promise<void> {
    return invokeNative("app_extension_location_reveal", z.null(), {
      scope,
      kind,
    }).then(() => undefined);
  },
  /**
   * Which file this scope writes its entries to. `null` for Skills, which keep
   * one directory per entry rather than one shared file.
   */
  describeLocation(
    scope: ExtensionScope,
    kind: ExtensionKind,
  ): Promise<ExtensionLocation | null> {
    return invokeNative(
      "app_extension_location_describe",
      extensionLocationSchema.nullable(),
      { scope, kind },
    );
  },

  /** Show the shared file in the file manager, or open it for editing. */
  openLocation(
    scope: ExtensionScope,
    kind: ExtensionKind,
    action: ExtensionLocationAction,
  ): Promise<void> {
    return invokeNative("app_extension_location_open", z.null(), {
      scope,
      kind,
      action: extensionLocationActionSchema.parse(action),
    }).then(() => undefined);
  },

  localInventory(): Promise<LocalExtensionInventory> {
    return invokeNative(
      "app_extensions_local_inventory",
      localExtensionInventorySchema,
    );
  },

  list(scope: ExtensionScope, kind: ExtensionKind): Promise<Extension[]> {
    return invokeNative("app_extensions_list", extensionListSchema, {
      scope,
      kind,
    });
  },

  /**
   * Toggle a single extension. The backend returns the refreshed full list along with
   * it -- the write path for all three extension kinds has cross-entry side effects
   * (switching to one prompt turns off the others for the same tool), so the frontend
   * never guesses the new state.
   */
  setEnabled(
    scope: ExtensionScope,
    kind: ExtensionKind,
    extension: string,
    enabled: boolean,
  ): Promise<Extension[]> {
    return invokeNative("app_extension_set_enabled", extensionListSchema, {
      scope,
      kind,
      extension,
      enabled,
    });
  },

  openDetectedSkillResource(
    scope: ExtensionScope,
    skill: string,
    action: DetectedSkillResourceAction,
  ): Promise<DetectedSkillResourceOpenOutcome> {
    return invokeNative(
      "app_detected_skill_resource_open",
      detectedSkillResourceOpenOutcomeSchema,
      {
        scope,
        skill: skillIdSchema.parse(skill),
        action: detectedSkillResourceActionSchema.parse(action),
      },
    );
  },

  /**
   * Copy a local skill to another tool. The outbound payload is only scope, target
   * tool id, and skill id -- the backend re-resolves the path from the local
   * inventory, so the renderer never has a path to hand over in the first place.
   */
  copyDetectedSkill(
    scope: ExtensionScope,
    target: ToolId,
    skill: string,
  ): Promise<Extension[]> {
    return invokeNative("app_detected_skill_copy", extensionListSchema, {
      scope,
      target,
      skill: skillIdSchema.parse(skill),
    });
  },

  /**
   * Adopt every detected item in one visible scope. The renderer submits no
   * connection spec, path, command, environment value, or credential.
   */
  adoptDetected(
    scope: ExtensionScope,
    kind: ExtensionKind,
  ): Promise<Extension[]> {
    return invokeNative("app_extensions_adopt_detected", extensionListSchema, {
      scope,
      kind,
    });
  },
};
