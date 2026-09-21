import {
  useMutation,
  useQueryClient,
  type UseMutationResult,
} from "@tanstack/react-query";
import { useTranslation } from "react-i18next";
import { toast } from "sonner";
import { operationKeys } from "@/entities/operation";
import { native, type ExtensionScope } from "@/native";

export interface RemoveMcpVariables {
  scope: ExtensionScope;
  mcpId: string;
  name: string;
}

export function useRemoveMcp(): UseMutationResult<
  string,
  Error,
  RemoveMcpVariables
> {
  const queryClient = useQueryClient();
  const { t } = useTranslation();

  return useMutation({
    mutationFn: ({ scope, mcpId }) => native.mcp.remove(scope, mcpId),
    onSuccess: (_operationId, { name }) => {
      toast.info(t("extensions.mcp.remove.queued", { name }), {
        description: t("extensions.mcp.remove.queuedDescription"),
      });
      return queryClient.invalidateQueries({ queryKey: operationKeys.all });
    },
  });
}
