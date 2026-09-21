import { z } from "zod";

/**
 * The two identity enums, in one module because each schema file needs the
 * other's. A tool carries the desktop application that shares its
 * configuration, and a desktop application carries the tool it relates to, so
 * defining them where they are used would make the two files import each other
 * and leave whichever loaded second reading an uninitialised binding.
 *
 * These strings reach the database, i18n keys and the wire format. They are
 * stable and must match `ToolId` and `DesktopAppId` in the Rust domain.
 */
export const toolIdSchema = z.enum([
  "claude-code",
  "codex",
  "opencode",
  "gemini-cli",
  "grok-build",
  "openclaw",
  "hermes",
  "pi",
  "kimi-code",
  "deepseek-dsh",
]);

export const desktopAppIdSchema = z.enum([
  "codex-app",
  "claude-desktop",
  "cursor",
  "zcode",
  "cherry-studio",
]);
