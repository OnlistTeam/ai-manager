import {
  useMutation,
  useQueryClient,
  type UseMutationResult,
} from "@tanstack/react-query";
import { extensionKeys, type Extension } from "@/entities/extension";
import { healthKeys } from "@/entities/health";
import {
  extensionScopeKey,
  native,
  type ExtensionKind,
  type ExtensionScope,
} from "@/native";

export interface ExtensionTarget {
  scope: ExtensionScope;
  kind: ExtensionKind;
  extensionId: string;
}

export interface SetExtensionEnabledVariables extends ExtensionTarget {
  enabled: boolean;
}

export interface OpenDetectedSkillResourceVariables {
  scope: ExtensionScope;
  skillId: string;
  action: "browse" | "edit";
}

export interface AdoptDetectedVariables {
  kind: ExtensionKind;
  extensionId: string;
  /** Every app the item was found in; each is adopted in turn. */
  found: readonly ExtensionScope[];
  /** The app whose switch was clicked, and the state asked for there. */
  scope: ExtensionScope;
  enabled: boolean;
}

/**
 * This feature has only one write operation, so it doesn't reach for a
 * generic helper. The only thing it shares with provider-management is
 * failure presentation — the whole project has a single error decomposition
 * point, `nativeError` (§42).
 */
export function useSetExtensionEnabled(): UseMutationResult<
  Extension[],
  Error,
  SetExtensionEnabledVariables
> {
  const queryClient = useQueryClient();

  return useMutation<Extension[], Error, SetExtensionEnabledVariables>({
    mutationFn: ({ scope, kind, extensionId, enabled }) =>
      native.extensions.setEnabled(scope, kind, extensionId, enabled),
    onSuccess: (data, { scope, kind }) => {
      // The backend response is the authoritative list: switching one entry
      // may turn off other entries for the same tool, so patching only this
      // one would leave the cache out of sync with real state.
      queryClient.setQueryData(
        extensionKeys.list(extensionScopeKey(scope), kind),
        data,
      );
      if (kind === "mcp") {
        void queryClient.invalidateQueries({ queryKey: healthKeys.snapshots });
      }
    },
    onError: async (_error, { scope, kind }) => {
      // Upstream this step writes both the DB and the tool's own config
      // file, so if it fails partway through, the toggle state in the cache
      // may already be stale. Only put the card into a retryable error state
      // once the authoritative list has been re-read.
      const refreshes = [
        queryClient.invalidateQueries({
          queryKey: extensionKeys.list(extensionScopeKey(scope), kind),
        }),
      ];
      if (kind === "mcp") {
        refreshes.push(
          queryClient.invalidateQueries({ queryKey: healthKeys.snapshots }),
        );
      }
      await Promise.all(refreshes);
    },
  });
}

export function useOpenDetectedSkillResource(): UseMutationResult<
  "folderOpened" | "editorOpened",
  Error,
  OpenDetectedSkillResourceVariables
> {
  return useMutation({
    mutationFn: ({ scope, skillId, action }) =>
      native.extensions.openDetectedSkillResource(scope, skillId, action),
  });
}

/**
 * A switch on a found item: the item is taken over from every app it was
 * found in, then the clicked app is set as asked. The calls run one after
 * another: the first brings the item under management and each later one only
 * sets its own app, so a failure part-way leaves a state the next attempt
 * simply continues. Every list is re-read afterwards, because taking an item
 * over changes its row in all of them, not just the ones called.
 */
export function useAdoptDetected(): UseMutationResult<
  void,
  Error,
  AdoptDetectedVariables
> {
  const queryClient = useQueryClient();

  return useMutation<void, Error, AdoptDetectedVariables>({
    mutationFn: async ({ kind, extensionId, found, scope, enabled }) => {
      for (const source of found) {
        await native.extensions.adoptDetected(source, kind, extensionId, true);
      }
      await native.extensions.adoptDetected(scope, kind, extensionId, enabled);
    },
    onSettled: (_data, _error, { kind }) =>
      Promise.all([
        queryClient.invalidateQueries({ queryKey: extensionKeys.all }),
        kind === "mcp"
          ? queryClient.invalidateQueries({ queryKey: healthKeys.snapshots })
          : undefined,
      ]),
  });
}
