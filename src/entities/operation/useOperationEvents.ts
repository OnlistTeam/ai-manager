import { useEffect } from "react";
import { useQueryClient } from "@tanstack/react-query";
import { onOperationChanged } from "@/native/events";
import type { Operation } from "@/native/schemas/operation";
import { toolKeys } from "@/entities/tool/keys";
import type { Tool } from "@/native/schemas/tool";
import { extensionKeys } from "@/entities/extension/keys";
import {
  extensionScopeKey,
  toolExtensionScope,
} from "@/native/schemas/extension";
import { healthKeys } from "@/entities/health/keys";
import { writeProviderConnectivityBatch } from "@/entities/health/queries";
import { skillCatalogKeys } from "@/entities/skill-catalog/keys";
import { skillBackupKeys } from "@/entities/skill-backup/keys";
import { skillUpdateKeys, type SkillUpdate } from "@/entities/skill-update";
import { operationKeys } from "./keys";
import { isTerminal, mergeOperation } from "./operationCache";

/**
 * The **single** app-wide subscription point for operation events (mounted on
 * AppRoot). If every consuming component subscribed on its own, the same
 * push would get written into the cache N times.
 *
 * Only a terminal state invalidates the tool list: whether an install/
 * uninstall actually succeeded is decided only by the backend re-running
 * detect (see plan decisions 3 and 4). Invalidating on every running update
 * would mean rescanning the disk once a second.
 */
export function useOperationEvents(): void {
  const queryClient = useQueryClient();

  useEffect(() => {
    let unlisten: (() => void) | undefined;
    let cancelled = false;

    void onOperationChanged((operation: Operation) => {
      queryClient.setQueryData<Operation[]>(operationKeys.list(), (current) =>
        mergeOperation(current ?? [], operation),
      );
      if (
        operation.tool !== null &&
        operation.output?.kind === "providerTests"
      ) {
        writeProviderConnectivityBatch(
          queryClient,
          operation.tool,
          operation.output.results,
        );
      }
      if (isTerminal(operation.status)) {
        if (operation.extension) {
          const sharedInventory =
            operation.extension.kind === "skill" ||
            operation.extension.kind === "mcp";
          // Per-tool lists are keyed by extension scope, the same shape the
          // prompt writers use; a task without a tool has no narrower list
          // to refresh than all of them.
          void queryClient.invalidateQueries({
            queryKey:
              sharedInventory || operation.tool === null
                ? extensionKeys.all
                : extensionKeys.list(
                    extensionScopeKey(toolExtensionScope(operation.tool)),
                    operation.extension.kind,
                  ),
          });
          if (operation.extension.kind === "skill") {
            void queryClient.invalidateQueries({
              queryKey: skillBackupKeys.all,
            });
            void queryClient.invalidateQueries({
              queryKey: skillCatalogKeys.all,
            });
            if (operation.status === "success") {
              queryClient.setQueryData<SkillUpdate[]>(
                skillUpdateKeys.list(),
                (current) =>
                  current?.filter(
                    (update) => update.id !== operation.extension?.id,
                  ),
              );
            }
          }
          if (operation.extension.kind === "mcp") {
            void queryClient.invalidateQueries({
              queryKey: healthKeys.snapshots,
            });
          }
        } else if (operation.kind !== "testProviders") {
          const verified =
            operation.output?.kind === "toolInventory"
              ? operation.output.tool
              : null;
          // A finished action already detected and verified the tool before it
          // was allowed to succeed. Landing that result keeps the card honest
          // immediately; re-scanning would first ask every registry for its
          // latest version, and until it answers the card shows a "done" panel
          // over the old version.
          if (verified && writeVerifiedTool(queryClient, verified)) {
            void queryClient.invalidateQueries({
              predicate: (query) =>
                query.queryKey[0] === toolKeys.all[0] &&
                query.queryKey[1] !== toolKeys.list()[1],
            });
          } else {
            void queryClient.invalidateQueries({ queryKey: toolKeys.all });
          }
        }
      }
    }).then((stop) => {
      // The subscription is async: the component may unmount before listen lands.
      if (cancelled) stop();
      else unlisten = stop;
    });

    return () => {
      cancelled = true;
      unlisten?.();
    };
  }, [queryClient]);
}

/**
 * Replaces one tool in the cached inventory in place. Returns false when there
 * is nothing to land in — an inventory that was never loaded must be fetched,
 * not invented from a single tool.
 */
function writeVerifiedTool(
  queryClient: ReturnType<typeof useQueryClient>,
  verified: Tool,
): boolean {
  const current = queryClient.getQueryData<Tool[]>(toolKeys.list());
  if (!current?.some((tool) => tool.id === verified.id)) return false;
  queryClient.setQueryData<Tool[]>(toolKeys.list(), (tools) =>
    tools?.map((tool) => (tool.id === verified.id ? verified : tool)),
  );
  return true;
}
