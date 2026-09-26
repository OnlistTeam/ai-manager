import { useMutation, useQueryClient } from "@tanstack/react-query";
import { useTranslation } from "react-i18next";
import { toast } from "sonner";
import { providerKeys, type Provider } from "@/entities/provider";
import { routingKeys } from "@/entities/routing";
import { native, type ToolId } from "@/native";
import { toErrorCopy } from "@/shared/lib/nativeError";

export interface ReorderProvidersVariables {
  tool: ToolId;
  /** Every service of this tool, in the order the user wants to see them. */
  providerIds: string[];
}

interface ReorderContext {
  previous: Provider[] | undefined;
}

/** Puts the cached services in the given order; ids it does not know are ignored. */
export function orderProviders(
  providers: readonly Provider[],
  providerIds: readonly string[],
): Provider[] {
  const byId = new Map(providers.map((provider) => [provider.id, provider]));
  return providerIds.flatMap((id) => {
    const provider = byId.get(id);
    return provider ? [provider] : [];
  });
}

/**
 * The new order shows at once and is saved in the background. A failed save
 * puts the old order back, says so, and rereads the list in case it changed
 * underneath.
 */
export function useReorderProviders() {
  const queryClient = useQueryClient();
  const { t } = useTranslation();

  return useMutation<
    Provider[],
    Error,
    ReorderProvidersVariables,
    ReorderContext
  >({
    mutationFn: ({ tool, providerIds }) =>
      native.providers.reorder(tool, providerIds),
    onMutate: async ({ tool, providerIds }) => {
      const key = providerKeys.list(tool);
      await queryClient.cancelQueries({ queryKey: key });
      const previous = queryClient.getQueryData<Provider[]>(key);
      if (previous) {
        queryClient.setQueryData(key, orderProviders(previous, providerIds));
      }
      return { previous };
    },
    onSuccess: (providers, { tool }) => {
      queryClient.setQueryData(providerKeys.list(tool), providers);
      // The failover queue follows the same order.
      void queryClient.invalidateQueries({ queryKey: routingKeys.all });
    },
    onError: (error, { tool }, context) => {
      if (context?.previous) {
        queryClient.setQueryData(providerKeys.list(tool), context.previous);
      }
      void queryClient.invalidateQueries({
        queryKey: providerKeys.list(tool),
      });
      const copy = toErrorCopy(error);
      toast.error(t(copy.messageKey), {
        description: copy.remediationKey ? t(copy.remediationKey) : undefined,
      });
    },
  });
}
