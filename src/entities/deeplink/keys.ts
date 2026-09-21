export const deepLinkKeys = {
  all: ["deeplink"] as const,
  pending: () => ["deeplink", "pending"] as const,
};
