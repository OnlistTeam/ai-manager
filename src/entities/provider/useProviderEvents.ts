import { useEffect } from "react";
import { useQueryClient } from "@tanstack/react-query";
import { onProviderChanged } from "@/native/events";
import { providerKeys } from "./keys";

/** Single app-level bridge for provider changes initiated outside the webview. */
export function useProviderEvents(): void {
  const queryClient = useQueryClient();

  useEffect(() => {
    let unlisten: (() => void) | undefined;
    let cancelled = false;
    void onProviderChanged(({ tool }) => {
      void queryClient.invalidateQueries({ queryKey: providerKeys.list(tool) });
      void queryClient.invalidateQueries({
        queryKey: providerKeys.runtimeContext(tool),
      });
    }).then((stop) => {
      if (cancelled) stop();
      else unlisten = stop;
    });

    return () => {
      cancelled = true;
      unlisten?.();
    };
  }, [queryClient]);
}
