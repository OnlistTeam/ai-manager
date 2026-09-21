import {
  useMutation,
  useQueryClient,
  type UseMutationResult,
} from "@tanstack/react-query";
import { useTranslation } from "react-i18next";
import { toast } from "sonner";
import { operationKeys } from "@/entities/operation";
import { native, type ExtensionScope, type McpInstallDraft } from "@/native";

export interface InstallMcpVariables {
  scope: ExtensionScope;
  draft: McpInstallDraft;
}

export function useInstallMcp(): UseMutationResult<
  string,
  Error,
  InstallMcpVariables
> {
  const queryClient = useQueryClient();
  const { t } = useTranslation();

  return useMutation({
    mutationFn: ({ scope, draft }) => native.mcp.install(scope, draft),
    onSuccess: (_operationId, { draft }) => {
      void queryClient.invalidateQueries({ queryKey: operationKeys.all });
      toast.info(t("extensions.mcp.install.queued", { name: draft.name }), {
        description: t("extensions.mcp.install.queuedDescription"),
      });
    },
  });
}
