import { z } from "zod";
import { extensionKindSchema, extensionScopeSchema } from "./extension";
import { toolIdSchema } from "./tool";

const currentDownloadStrategySchema = z.literal("automatic");

/**
 * Earlier development builds emitted either `officialOnly` or the
 * region-named `chinaResilient`. Accept both at the IPC boundary, but never
 * keep or emit either legacy value in current product state.
 */
export const downloadStrategySchema = z.preprocess(
  (value) =>
    value === "officialOnly" || value === "chinaResilient"
      ? "automatic"
      : value,
  currentDownloadStrategySchema,
);

/** Mirrors Rust `domain::TerminalAppId` item for item. */
export const terminalAppIdSchema = z.enum([
  "system",
  "iterm2",
  "ghostty",
  "kitty",
  "wezterm",
  "alacritty",
]);

/**
 * Mirrors Rust `domain::ProductSettings` field for field.
 * `Option<T>` becomes `.nullable()` (**not** `.optional()`): the Rust side has no
 * `skip_serializing_if`, so `None` always serializes to `null`.
 */
export const productSettingsSchema = z.object({
  advancedMode: z.boolean(),
  importPromptSeen: z.boolean(),
  toolScope: toolIdSchema.nullable(),
  extensionScope: extensionScopeSchema.nullable().default(null),
  extensionKind: extensionKindSchema.nullable(),
  // Older dev backends did not emit this field; production Rust always does.
  downloadStrategy: downloadStrategySchema.default("automatic"),
  // Older dev backends did not emit this field. The conservative migration is off.
  automaticProviderFailover: z.boolean().default(false),
  // Null until a terminal has ever been chosen; the platform layer falls back to the system default.
  terminalApp: terminalAppIdSchema.nullable().default(null),
});

export type ProductSettings = z.infer<typeof productSettingsSchema>;
export type TerminalAppId = z.infer<typeof terminalAppIdSchema>;
export type DownloadStrategy = z.infer<typeof downloadStrategySchema>;

/**
 * Field-for-field identical to Rust's `ProductSettings::default()`. Only used as a
 * fallback in the narrow window before settings have been read and something must
 * already decide what to render (AppRoot's readiness gate keeps the normal path from
 * ever needing it).
 */
export const DEFAULT_PRODUCT_SETTINGS: ProductSettings = {
  advancedMode: false,
  importPromptSeen: false,
  toolScope: null,
  extensionScope: null,
  extensionKind: null,
  downloadStrategy: "automatic",
  automaticProviderFailover: false,
  terminalApp: null,
};
