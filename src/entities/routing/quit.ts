import { useEffect, useState } from "react";
import { useMutation, type UseMutationResult } from "@tanstack/react-query";
import { native, onQuitRequested, type RoutedTool } from "@/native";

export interface QuitRequest {
  /** The routed tools to name, or `null` while no quit is waiting. */
  tools: RoutedTool[] | null;
  /** The user chose to keep AI Manager running. */
  dismiss: () => void;
}

/**
 * A quit held because tools are routed through AI Manager (ADR-0054). The
 * question is transient UI state: answering or dismissing it clears it, and
 * the next quit request asks again.
 */
export function useQuitRequest(): QuitRequest {
  const [tools, setTools] = useState<RoutedTool[] | null>(null);

  useEffect(() => {
    const subscription = onQuitRequested((payload) => setTools(payload.tools));
    subscription.catch((error: unknown) => {
      console.error("Could not listen for quit requests", error);
    });
    return () => {
      void subscription.then(
        (unlisten) => unlisten(),
        () => undefined,
      );
    };
  }, []);

  return { tools, dismiss: () => setTools(null) };
}

export function useConfirmQuit(): UseMutationResult<null, Error, void> {
  return useMutation({ mutationFn: () => native.routing.confirmQuit() });
}
