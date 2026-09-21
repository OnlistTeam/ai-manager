export const providerKeys = {
  all: ["providers"] as const,
  list: (tool: string) => ["providers", "list", tool] as const,
  connectionProfile: (tool: string) =>
    ["providers", "connection-profile", tool] as const,
  editProfile: (tool: string, provider: string) =>
    ["providers", "edit-profile", tool, provider] as const,
  editProfiles: (tool: string) => ["providers", "edit-profile", tool] as const,
  runtimeContext: (tool: string) =>
    ["providers", "runtime-context", tool] as const,
};
