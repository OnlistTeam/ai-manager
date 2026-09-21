export const extensionKeys = {
  all: ["extensions"] as const,
  localInventory: () => ["extensions", "local-inventory"] as const,
  list: (scope: string, kind: string) =>
    ["extensions", "list", scope, kind] as const,
};
