export const toolKeys = {
  all: ["tools"] as const,
  list: () => ["tools", "list"] as const,
  versionCheck: () => ["tools", "version-check"] as const,
  updatePreview: (tools: readonly string[], revision = 0) =>
    ["tools", "update-preview", [...tools].sort(), revision] as const,
  versions: (tool: string) => ["tools", "versions", tool] as const,
  uninstallPreview: (tool: string) =>
    ["tools", "uninstall-preview", tool] as const,
};
