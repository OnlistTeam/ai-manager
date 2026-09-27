import {
  useMutation,
  useQueryClient,
  type UseMutationResult,
} from "@tanstack/react-query";
import { useTranslation } from "react-i18next";
import { toast } from "sonner";
import { providerKeys, type ToolModelChoice } from "@/entities/provider";
import { native, type ToolId } from "@/native";
import { toErrorCopy } from "@/shared/lib/nativeError";

export interface SetToolModelVariables {
  tool: ToolId;
  /** The saved endpoint the model belongs to; `null` for the connection in force. */
  providerId: string | null;
  /** `null` removes the model so the tool decides. */
  model: string | null;
}

export interface SetToolEffortVariables {
  tool: ToolId;
  /** `null` removes the effort so the tool decides. */
  effort: string | null;
}

/**
 * Both writes return the tool's model choice as it now stands, which replaces
 * the cached one; a failure re-reads it, because a write that failed halfway
 * may still have changed the file.
 */
function useModelChoiceMutation<TVariables extends { tool: ToolId }>(
  run: (variables: TVariables) => Promise<ToolModelChoice>,
  onDone?: (variables: TVariables) => void,
): UseMutationResult<ToolModelChoice, Error, TVariables> {
  const { t } = useTranslation();
  const queryClient = useQueryClient();
  return useMutation<ToolModelChoice, Error, TVariables>({
    mutationFn: run,
    onSuccess: (choice, variables) => {
      queryClient.setQueryData(
        providerKeys.modelChoice(variables.tool),
        choice,
      );
      onDone?.(variables);
    },
    onError: (error, variables) => {
      void queryClient.invalidateQueries({
        queryKey: providerKeys.modelChoice(variables.tool),
      });
      const copy = toErrorCopy(error);
      toast.error(t(copy.messageKey), {
        description: copy.remediationKey ? t(copy.remediationKey) : undefined,
      });
    },
  });
}

/** Sets the model saved with an endpoint, and in the file while it is in use (ADR-0054). */
export function useSetToolModel(): UseMutationResult<
  ToolModelChoice,
  Error,
  SetToolModelVariables
> {
  const queryClient = useQueryClient();
  return useModelChoiceMutation<SetToolModelVariables>(
    ({ tool, providerId, model }) =>
      native.modelChoice.setModel(tool, providerId, model),
    ({ tool, providerId }) => {
      if (providerId === null) return;
      // The endpoint's edit form and its pinned-model label read this.
      void queryClient.invalidateQueries({
        queryKey: providerKeys.editProfile(tool, providerId),
      });
    },
  );
}

/** Sets how hard the tool thinks; a later switch keeps it (ADR-0054). */
export function useSetToolEffort(): UseMutationResult<
  ToolModelChoice,
  Error,
  SetToolEffortVariables
> {
  return useModelChoiceMutation<SetToolEffortVariables>(({ tool, effort }) =>
    native.modelChoice.setEffort(tool, effort),
  );
}
