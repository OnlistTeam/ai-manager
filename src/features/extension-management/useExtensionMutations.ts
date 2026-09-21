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
  toolExtensionScope,
  type ExtensionKind,
  type ExtensionScope,
  type ToolId,
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

export interface CopyDetectedSkillVariables {
  scope: ExtensionScope;
  target: ToolId;
  skillId: string;
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
 * Copy a local skill to another tool. The copy is an independent replica, so
 * three caches are affected: the target tool's skill list (the backend
 * returns the authoritative result, written directly), the source tool's list
 * (it now has a sibling that "also has this skill"), and the local inventory
 * (the card's ownership row reads from it).
 */
export function useCopyDetectedSkill(): UseMutationResult<
  Extension[],
  Error,
  CopyDetectedSkillVariables
> {
  const queryClient = useQueryClient();

  return useMutation<Extension[], Error, CopyDetectedSkillVariables>({
    mutationFn: ({ scope, target, skillId }) =>
      native.extensions.copyDetectedSkill(scope, target, skillId),
    onSuccess: (data, { scope, target }) => {
      queryClient.setQueryData(
        extensionKeys.list(
          extensionScopeKey(toolExtensionScope(target)),
          "skill",
        ),
        data,
      );
      void queryClient.invalidateQueries({
        queryKey: extensionKeys.list(extensionScopeKey(scope), "skill"),
      });
      void queryClient.invalidateQueries({
        queryKey: extensionKeys.localInventory(),
      });
    },
    onError: async (_error, { scope, target }) => {
      // If the write fails partway through, part of it may already be on
      // disk. Re-read all three caches so the UI shows real state instead of
      // guessing.
      await Promise.all([
        queryClient.invalidateQueries({
          queryKey: extensionKeys.list(
            extensionScopeKey(toolExtensionScope(target)),
            "skill",
          ),
        }),
        queryClient.invalidateQueries({
          queryKey: extensionKeys.list(extensionScopeKey(scope), "skill"),
        }),
        queryClient.invalidateQueries({
          queryKey: extensionKeys.localInventory(),
        }),
      ]);
    },
  });
}
