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
  /**
   * Under the runtime context on purpose: every switch, save or outside
   * change that re-reads what a tool uses also re-reads its model and effort
   * (ADR-0054).
   */
  modelChoice: (tool: string) =>
    ["providers", "runtime-context", tool, "model-choice"] as const,
};
