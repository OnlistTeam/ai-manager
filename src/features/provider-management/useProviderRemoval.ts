import { useMutation, useQueryClient } from "@tanstack/react-query";
import { useTranslation } from "react-i18next";
import { toast } from "sonner";
import { clearProviderConnectivity, healthKeys } from "@/entities/health";
import { providerKeys, type Provider } from "@/entities/provider";
import { native, type ToolId } from "@/native";

export interface RemoveProviderVariables {
  tool: ToolId;
  providerId: string;
  name: string;
}

export function useRemoveProvider() {
  const queryClient = useQueryClient();
  const { t } = useTranslation();

  return useMutation<Provider[], Error, RemoveProviderVariables>({
    mutationFn: ({ tool, providerId }) =>
      native.providers.remove(tool, providerId),
    onSuccess: (providers, { tool, providerId, name }) => {
      queryClient.setQueryData(providerKeys.list(tool), providers);
      clearProviderConnectivity(queryClient, tool, providerId);
      void queryClient.invalidateQueries({ queryKey: healthKeys.snapshots });
      toast.success(t("services.remove.removed", { name }));
    },
    onError: async (_error, { tool }) => {
      await Promise.all([
        queryClient.invalidateQueries({
          queryKey: providerKeys.list(tool),
        }),
        queryClient.invalidateQueries({ queryKey: healthKeys.snapshots }),
      ]);
    },
  });
}
