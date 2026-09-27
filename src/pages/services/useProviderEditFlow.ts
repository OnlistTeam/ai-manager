import { useState } from "react";
import { useTranslation } from "react-i18next";
import { toast } from "sonner";
import {
  useProviderEditProfile,
  type Provider,
  type ProviderDraft,
  type ToolId,
} from "@/entities/provider";
import { restartNoteKey } from "@/entities/routing";
import { useSaveProvider } from "@/features/provider-management";

export function useProviderEditFlow(tool: ToolId | null, toolName: string) {
  const { t } = useTranslation();
  const [provider, setProvider] = useState<Provider | null>(null);
  const mutation = useSaveProvider();
  const profile = useProviderEditProfile(tool, provider?.id ?? null);

  function close() {
    setProvider(null);
    mutation.reset();
  }

  function open(nextProvider: Provider) {
    mutation.reset();
    setProvider(nextProvider);
  }

  function setOpen(open: boolean) {
    if (!open) close();
  }

  function submit(draft: ProviderDraft) {
    if (provider === null || tool === null) return;
    const providerId = provider.id;
    mutation.mutate(
      { tool, providerId, draft },
      {
        onSuccess: (saved) => {
          setProvider(null);
          // The edit left the endpoint unable to go through AI Manager, so
          // the tool's route ended (ADR-0054).
          if (saved.routingEnded) {
            const endpoint =
              saved.providers.find((item) => item.id === providerId)?.name ??
              draft.name;
            toast.info(t("routing.ended", { name: toolName, endpoint }), {
              description: t(restartNoteKey(saved.routingEnded, false), {
                name: toolName,
              }),
            });
          }
        },
      },
    );
  }

  return {
    provider,
    busy: mutation.isPending,
    error: mutation.error,
    profile,
    open,
    close,
    setOpen,
    resetError: mutation.reset,
    submit,
  };
}
