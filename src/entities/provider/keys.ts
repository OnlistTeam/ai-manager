export const providerKeys = {
  all: ["providers"] as const,
  list: (tool: string) => ["providers", "list", tool] as const,
  connectionProfile: (tool: string) =>
    ["providers", "connection-profile", tool] as const,
  editProfile: (tool: string, provider: string) =>
    ["providers", "edit-profile", tool, provider] as const,
  editProfiles: (tool: string) => ["providers", "edit-profile", tool] as const,
  /**
   * Every model catalogue read for one tool: saved entries and the address in
   * force. A save refreshes them all, since it may change the address or key.
   */
  modelCatalogs: (tool: string) =>
    ["providers", "model-catalog", tool] as const,
  runtimeContext: (tool: string) =>
    ["providers", "runtime-context", tool] as const,
  /**
   * Under the runtime context on purpose: every switch, save or outside
   * change that re-reads what a tool uses also re-reads its model and effort
   * (ADR-0055).
   */
  modelChoice: (tool: string) =>
    ["providers", "runtime-context", tool, "model-choice"] as const,
  /** Under the runtime context too: a switch can move Codex's login aside. */
  loginStatus: (tool: string) =>
    ["providers", "runtime-context", tool, "login-status"] as const,
};
