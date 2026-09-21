import {
  useMutation,
  useQuery,
  useQueryClient,
  type UseMutationResult,
  type UseQueryResult,
} from "@tanstack/react-query";

import { providerKeys } from "@/entities/provider";
import {
  native,
  type ShellVariableLocation,
  type ShellVariableUpdate,
  type ShellVariableWritten,
  type ToolId,
} from "@/native";

/**
 * The start-up lines behind a connection the shell sets (ADR-0042).
 *
 * Read on demand rather than with the runtime context: it walks the profile
 * tree, and the services page should not pay for that on every visit when most
 * tools are configured through their own files.
 */

const shellVariableKey = (tool: ToolId | null) =>
  [...providerKeys.all, "shell-variables", tool ?? ""] as const;

export function useShellVariables(
  tool: ToolId | null,
  enabled = true,
): UseQueryResult<ShellVariableLocation[], Error> {
  return useQuery({
    queryKey: shellVariableKey(tool),
    queryFn: () =>
      tool === null
        ? Promise.reject(new Error("A tool is required"))
        : native.providers.locateShellVariables(tool),
    enabled: tool !== null && enabled,
    // The answer changes when the user edits a dotfile outside this app, which
    // is exactly when a stale answer would be wrong, so it is not cached long.
    staleTime: 15_000,
    retry: false,
  });
}

export interface WriteShellVariableVariables {
  tool: ToolId;
  update: ShellVariableUpdate;
}

/**
 * Replaces one value on one line.
 *
 * On success everything that reads the shell is invalidated: the runtime
 * context (which card is in use, and what it says), the located lines, and the
 * health snapshot. The login shell itself is re-probed by the backend on the
 * next read, so the card reflects the new value without a restart — but the
 * user's already-open terminals keep the old one, which the dialog says.
 */
export function useWriteShellVariable(): UseMutationResult<
  ShellVariableWritten,
  Error,
  WriteShellVariableVariables
> {
  const queryClient = useQueryClient();
  return useMutation<ShellVariableWritten, Error, WriteShellVariableVariables>({
    mutationFn: ({ tool, update }) =>
      native.providers.writeShellVariable(tool, update),
    onSuccess: (_written, { tool }) => {
      void queryClient.invalidateQueries({ queryKey: shellVariableKey(tool) });
      void queryClient.invalidateQueries({
        queryKey: providerKeys.runtimeContext(tool),
      });
      void queryClient.invalidateQueries({ queryKey: ["health"] });
    },
  });
}
