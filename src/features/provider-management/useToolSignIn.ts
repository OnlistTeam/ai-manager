import { useEffect, useRef, useState } from "react";
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { providerKeys } from "@/entities/provider";
import { healthKeys } from "@/entities/health";
import {
  native,
  type ProviderCreateResult,
  type SignInProgress,
  type ToolId,
} from "@/native";

/** How often a waiting sign-in is asked where it stands. */
const POLL_MS = 1000;

/**
 * One sign-in started from AI Manager (ADR-0061): start it, follow it until
 * the browser comes back, and hand the endpoint it became to `onDone`.
 * Closing the dialog cancels a sign-in still waiting.
 */
export function useToolSignIn(
  tool: ToolId,
  onDone: (result: ProviderCreateResult) => void,
) {
  const queryClient = useQueryClient();
  const [flowId, setFlowId] = useState<string | null>(null);
  const done = useRef(onDone);
  done.current = onDone;

  const start = useMutation({
    mutationFn: () => native.providers.signInStart(tool),
    onSuccess: (progress) => setFlowId(progress.id),
  });

  const status = useQuery({
    queryKey: ["providers", "sign-in", tool, flowId ?? ""] as const,
    queryFn: () => native.providers.signInStatus(tool, flowId ?? ""),
    enabled: flowId !== null,
    refetchInterval: (query) =>
      query.state.data?.phase === "waiting" || !query.state.data
        ? POLL_MS
        : false,
    refetchOnWindowFocus: false,
    retry: false,
    gcTime: 0,
  });

  const progress: SignInProgress | undefined =
    flowId === null
      ? undefined
      : (status.data ?? (start.data?.id === flowId ? start.data : undefined));

  const created = progress?.phase === "done" ? progress.created : null;
  useEffect(() => {
    if (!created) return;
    queryClient.setQueryData(providerKeys.list(tool), created.providers);
    void queryClient.invalidateQueries({
      queryKey: providerKeys.runtimeContext(tool),
    });
    void queryClient.invalidateQueries({ queryKey: healthKeys.snapshots });
    setFlowId(null);
    start.reset();
    done.current(created);
    // `start` is stable enough: only its reset is called, once per sign-in.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [created, queryClient, tool]);

  const cancel = () => {
    if (flowId !== null && progress?.phase === "waiting") {
      void native.providers.signInCancel(tool, flowId).catch(() => undefined);
    }
    setFlowId(null);
    start.reset();
  };

  return {
    /** `undefined` before a sign-in starts and after one is closed. */
    progress,
    starting: start.isPending,
    waiting: start.isPending || progress?.phase === "waiting",
    error: start.error ?? status.error,
    begin: () => {
      setFlowId(null);
      start.mutate();
    },
    cancel,
    openAgain: () => {
      if (flowId !== null) {
        void native.providers.signInOpen(tool, flowId).catch(() => undefined);
      }
    },
  };
}
