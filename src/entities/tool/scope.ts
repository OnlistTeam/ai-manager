import type { Tool } from "@/native";

/**
 * Whether a tool's configuration is worth showing a scope for.
 *
 * The obvious rule — "the command line is installed" — misses a real case. A
 * desktop application can own the same configuration directory as a command
 * line: the Codex application and the Codex CLI both use `~/.codex`. Someone
 * running only the application has services, MCP servers and prompts that are
 * genuinely in use, and hiding them means the product cannot manage a
 * configuration the user can see taking effect.
 *
 * `configurationSharedWith` is set by the backend from the *application's*
 * installed state, never from the presence of a directory, so uninstalling the
 * application removes the scope again.
 */
export function hasManageableConfiguration(tool: Tool): boolean {
  return tool.status !== "notInstalled" || tool.configurationSharedWith != null;
}
