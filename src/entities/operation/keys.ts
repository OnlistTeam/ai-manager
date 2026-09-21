export const operationKeys = {
  all: ["operations"] as const,
  list: () => ["operations", "list"] as const,
};
