import { lazy, Suspense, useEffect, useState } from "react";
import { useOperationEvents } from "@/entities/operation";
import { useProviderEvents } from "@/entities/provider";
import {
  useProductSettings,
  useSaveProductSettings,
} from "@/entities/settings";
import { useUpdateStatus } from "@/entities/update";
import { Toaster } from "@/shared/ui/Toaster";
import { StartupScreen } from "./StartupScreen";

const AppShell = lazy(async () => ({
  default: (await import("./AppShell")).AppShell,
}));
const ImportPromptModal = lazy(async () => ({
  default: (await import("@/features/import-existing")).ImportPromptModal,
}));
const DeepLinkImportBoundary = lazy(async () => ({
  default: (await import("@/features/deep-link-import")).DeepLinkImportBoundary,
}));

interface ImportPromptBoundaryProps {
  enabled: boolean;
  onResolve: () => Promise<void>;
}

/**
 * Existing users who already answered the import question never download the
 * modal stack. Once requested, keep it mounted so an optimistic settings save
 * cannot erase the modal's pending/error recovery state halfway through.
 */
function ImportPromptBoundary({
  enabled,
  onResolve,
}: ImportPromptBoundaryProps) {
  const [requested, setRequested] = useState(enabled);

  useEffect(() => {
    if (enabled) setRequested(true);
  }, [enabled]);

  if (!requested) return null;

  return (
    <Suspense fallback={null}>
      <ImportPromptModal enabled={enabled} onResolve={onResolve} />
    </Suspense>
  );
}

/**
 * The mount point for the product shell. Event subscriptions and the startup
 * update check live here, once for the whole app.
 *
 * **Readiness gate**: the shell is not rendered until product settings have
 * loaded. This establishes an invariant for the whole tree: by the time any
 * page mounts, the settings cache is **guaranteed** to be populated, so
 * `useSaveProductSettings`'s "merge into the cached copy" step always has a
 * baseline (decision 3). It also keeps the startup import question from being
 * decided against a value nobody has read yet.
 */
export function AppRoot() {
  useOperationEvents();
  useProviderEvents();
  useUpdateStatus();
  const settings = useProductSettings();
  const importDecision = useSaveProductSettings();

  const markImportPromptSeen = async () => {
    await importDecision.mutateAsync({ importPromptSeen: true });
  };

  if (settings.isPending) {
    return <StartupScreen />;
  }

  // A failed settings read has no safe full-record snapshot to save onto, so never
  // open a modal that cannot persist its answer. The next successful launch can
  // discover the existing setup again.
  const importPromptSeen =
    settings.isError || (settings.data?.importPromptSeen ?? false);

  return (
    <>
      <Suspense fallback={<StartupScreen />}>
        <AppShell />
      </Suspense>
      <ImportPromptBoundary
        enabled={!importPromptSeen}
        onResolve={markImportPromptSeen}
      />
      {/*
        A one-click import can arrive at any moment and from any page, so its
        dialog is mounted beside the shell rather than inside a page. The chunk
        is loaded on demand, never as part of the first paint (ADR-0029).
      */}
      <Suspense fallback={null}>
        <DeepLinkImportBoundary />
      </Suspense>
      <Toaster />
    </>
  );
}
