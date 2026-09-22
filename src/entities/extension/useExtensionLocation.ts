import { useMutation, useQuery } from "@tanstack/react-query";
import {
  native,
  type ExtensionKind,
  type ExtensionLocation,
  type ExtensionLocationAction,
  type ExtensionScope,
  extensionScopeKey,
} from "@/native";
import { extensionKeys } from "./keys";

/**
 * The file this scope's MCP servers or instructions are written in.
 *
 * `null` for Skills, which keep one directory per entry rather than one
 * shared file, and for a scope whose tool has no such file at all.
 *
 * Not cached for long: the answer changes when the tool is installed,
 * uninstalled, or first writes its config, and a stale "does not exist yet"
 * is exactly the case where a wrong answer is visible.
 */
export function useExtensionLocation(
  scope: ExtensionScope | null,
  kind: ExtensionKind,
) {
  return useQuery<ExtensionLocation | null, Error>({
    queryKey: [
      ...extensionKeys.all,
      "location",
      scope === null ? "" : extensionScopeKey(scope),
      kind,
    ],
    queryFn: () =>
      scope === null
        ? Promise.resolve(null)
        : native.extensions.describeLocation(scope, kind),
    enabled: scope !== null,
    staleTime: 15_000,
    retry: false,
  });
}

export interface OpenExtensionLocationVariables {
  scope: ExtensionScope;
  kind: ExtensionKind;
  action: ExtensionLocationAction;
}

/** Shows the shared file in the file manager, or opens it for editing. */
export function useOpenExtensionLocation() {
  return useMutation<void, Error, OpenExtensionLocationVariables>({
    mutationFn: ({ scope, kind, action }) =>
      native.extensions.openLocation(scope, kind, action),
  });
}
