import {
  useMutation,
  useQueryClient,
  type UseMutationResult,
} from "@tanstack/react-query";
import {
  extensionKeys,
  extensionScopeKey,
  toolExtensionScope,
  type Extension,
} from "@/entities/extension";
import { promptKeys } from "@/entities/prompt";
import { native, type PromptDraft, type ToolId } from "@/native";

export interface SavePromptVariables {
  tool: ToolId;
  promptId: string | null;
  draft: PromptDraft;
}

export interface RemovePromptVariables {
  tool: ToolId;
  promptId: string;
}

export interface ImportPromptVariables {
  tool: ToolId;
}

/**
 * The list query uses a scope key (`tool:claude-code`), not a bare ToolId —
 * writing back must compute the same key, otherwise `setQueryData` lands on
 * a cache entry nobody reads and the save succeeds while the UI doesn't move.
 */
function promptListKey(tool: ToolId) {
  return extensionKeys.list(
    extensionScopeKey(toolExtensionScope(tool)),
    "prompt",
  );
}

function useAuthoritativePromptMutation<TVariables>(
  mutationFn: (variables: TVariables) => Promise<Extension[]>,
  scope: (variables: TVariables) => { tool: ToolId; promptId?: string },
): UseMutationResult<Extension[], Error, TVariables> {
  const queryClient = useQueryClient();
  return useMutation({
    mutationFn,
    onSuccess: (data, variables) => {
      const { tool, promptId } = scope(variables);
      queryClient.setQueryData(promptListKey(tool), data);
      if (promptId) {
        void queryClient.invalidateQueries({
          queryKey: promptKeys.detail(tool, promptId),
        });
      }
    },
    onError: async (_error, variables) => {
      const { tool, promptId } = scope(variables);
      const refreshes = [
        queryClient.invalidateQueries({ queryKey: promptListKey(tool) }),
      ];
      if (promptId) {
        refreshes.push(
          queryClient.invalidateQueries({
            queryKey: promptKeys.detail(tool, promptId),
          }),
        );
      }
      await Promise.all(refreshes);
    },
  });
}

export function useSavePrompt(): UseMutationResult<
  Extension[],
  Error,
  SavePromptVariables
> {
  return useAuthoritativePromptMutation(
    ({ tool, promptId, draft }) => native.prompts.save(tool, promptId, draft),
    ({ tool, promptId }) => ({ tool, promptId: promptId ?? undefined }),
  );
}

export function useRemovePrompt(): UseMutationResult<
  Extension[],
  Error,
  RemovePromptVariables
> {
  return useAuthoritativePromptMutation(
    ({ tool, promptId }) => native.prompts.remove(tool, promptId),
    ({ tool, promptId }) => ({ tool, promptId }),
  );
}

export function useImportPrompt(): UseMutationResult<
  Extension[],
  Error,
  ImportPromptVariables
> {
  return useAuthoritativePromptMutation(
    ({ tool }) => native.prompts.importCurrent(tool),
    ({ tool }) => ({ tool }),
  );
}
