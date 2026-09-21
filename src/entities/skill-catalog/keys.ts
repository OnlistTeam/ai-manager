export const skillCatalogKeys = {
  all: ["skill-catalog"] as const,
  list: (tool: string) => ["skill-catalog", "list", tool] as const,
};
