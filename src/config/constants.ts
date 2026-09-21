// Provider type constants
export const PROVIDER_TYPES = {
  GITHUB_COPILOT: "github_copilot",
  CODEX_OAUTH: "codex_oauth",
  XAI_OAUTH: "xai_oauth",
} as const;

// Managed OAuth provider types: the real credential is injected by the local
// proxy on each request, so routing takeover must stay on regardless of
// whether the upstream also needs format conversion. Adding a new preset of
// this kind only requires appending its providerType here — the needsRouting
// check picks it up automatically, no per-case special-handling needed.
export const OAUTH_PROVIDER_TYPES: readonly string[] = [
  PROVIDER_TYPES.GITHUB_COPILOT,
  PROVIDER_TYPES.CODEX_OAUTH,
  PROVIDER_TYPES.XAI_OAUTH,
];

/** Whether a providerType is managed OAuth (credential injected by the proxy, routing required). */
export function isOAuthProviderType(
  providerType: string | null | undefined,
): boolean {
  return providerType != null && OAUTH_PROVIDER_TYPES.includes(providerType);
}

// Usage-script template type constants
export const TEMPLATE_TYPES = {
  CUSTOM: "custom",
  GENERAL: "general",
  NEW_API: "newapi",
  GITHUB_COPILOT: "github_copilot",
  TOKEN_PLAN: "token_plan",
  BALANCE: "balance",
  OFFICIAL_SUBSCRIPTION: "official_subscription",
} as const;

export type TemplateType = (typeof TEMPLATE_TYPES)[keyof typeof TEMPLATE_TYPES];
