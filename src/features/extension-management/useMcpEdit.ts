import {
  useMutation,
  useQuery,
  useQueryClient,
  type UseMutationResult,
  type UseQueryResult,
} from "@tanstack/react-query";
import { useTranslation } from "react-i18next";
import { toast } from "sonner";
import { extensionKeys } from "@/entities/extension";
import { healthKeys } from "@/entities/health";
import {
  extensionScopeKey,
  native,
  type ExtensionScope,
  type McpEditForm,
  type McpInstallDraft,
} from "@/native";

/** The connection a person asked to edit, as the row knows it. */
export interface McpEditTarget {
  id: string;
  name: string;
  /** Where to read it from: any app for a managed row, its own for a found one. */
  scope: ExtensionScope;
  /**
   * Every app a found connection was found in, so saving can take it over
   * first. Empty for a managed connection.
   */
  found: readonly ExtensionScope[];
}

export interface UpdateMcpVariables {
  target: McpEditTarget;
  draft: McpInstallDraft;
}

const mcpEditKey = (target: McpEditTarget | null) =>
  [
    "mcp",
    "edit",
    target ? extensionScopeKey(target.scope) : "",
    target?.id ?? "",
  ] as const;

/**
 * The saved connection for the edit form. Values can be credentials, so the
 * result is read fresh each time the form opens and dropped as soon as it
 * closes rather than kept in the session cache.
 */
export function useMcpEditForm(
  target: McpEditTarget | null,
  open: boolean,
): UseQueryResult<McpEditForm, Error> {
  return useQuery({
    queryKey: mcpEditKey(target),
    queryFn: () =>
      target
        ? native.mcp.get(target.scope, target.id)
        : Promise.reject(new Error("MCP edit target is missing")),
    enabled: open && target !== null,
    staleTime: 0,
    gcTime: 0,
    retry: false,
  });
}

/**
 * Save an edited connection in place. A found connection is taken over first,
 * from every app it was found in, the way its first switch click would; the
 * save then replaces it and rewrites every app where it is on.
 */
export function useUpdateMcp(): UseMutationResult<
  void,
  Error,
  UpdateMcpVariables
> {
  const queryClient = useQueryClient();
  const { t } = useTranslation();

  return useMutation<void, Error, UpdateMcpVariables>({
    mutationFn: async ({ target, draft }) => {
      for (const source of target.found) {
        await native.extensions.adoptDetected(source, "mcp", target.id, true);
      }
      await native.mcp.update(target.scope, target.id, draft);
    },
    onSuccess: (_data, { draft }) => {
      toast.success(t("extensions.mcp.edit.saved", { name: draft.name }));
    },
    onSettled: () =>
      Promise.all([
        queryClient.invalidateQueries({ queryKey: extensionKeys.all }),
        queryClient.invalidateQueries({ queryKey: healthKeys.snapshots }),
      ]),
  });
}
