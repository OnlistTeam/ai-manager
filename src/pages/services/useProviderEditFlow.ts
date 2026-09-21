import { useState } from "react";
import {
  useProviderEditProfile,
  type Provider,
  type ProviderDraft,
  type ToolId,
} from "@/entities/provider";
import { useSaveProvider } from "@/features/provider-management";

export function useProviderEditFlow(tool: ToolId | null) {
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
    mutation.mutate(
      { tool, providerId: provider.id, draft },
      { onSuccess: () => setProvider(null) },
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
