import { useState } from "react";
import type { DesktopApp } from "@/entities/desktop-app";
import {
  extensionScopeKey,
  toolExtensionScope,
  type ExtensionKind,
  type ExtensionScope,
} from "@/entities/extension";
import {
  useProductSettings,
  useSaveProductSettings,
  type ProductSettings,
} from "@/entities/settings";
import type { Tool } from "@/entities/tool";
import { resolveExtensionScope } from "@/features/extension-management";

/**
 * Keeps both extension filters usable when remembering either one fails. The
 * settings Query remains the only persisted state; local values only preserve
 * explicit choices for the current visit while a failed write is recoverable.
 */
export function useExtensionScope(
  tools: readonly Tool[],
  desktopApps: readonly DesktopApp[],
  scopesReady: boolean,
  preferredScope: ExtensionScope | null = null,
  preferredKind: ExtensionKind | null = null,
  fixedKind: ExtensionKind | null = null,
) {
  const settings = useProductSettings();
  const save = useSaveProductSettings();
  const [localKind, setLocalKind] = useState<ExtensionKind | null>(null);
  const [localScope, setLocalScope] = useState<ExtensionScope | null>(null);
  const [failedKind, setFailedKind] = useState<ExtensionKind | null>(null);
  const [failedScope, setFailedScope] = useState<ExtensionScope | null>(null);
  const [routeScope, setRouteScope] = useState<ExtensionScope | null>(
    preferredScope,
  );
  const [routeKind, setRouteKind] = useState<ExtensionKind | null>(
    preferredKind,
  );

  const displayedKind =
    fixedKind ??
    localKind ??
    routeKind ??
    (routeScope?.kind === "desktopApp" ? "mcp" : null) ??
    settings.data?.extensionKind ??
    null;
  const rememberedTool = settings.data?.toolScope ?? null;
  const displayedScope =
    localScope ??
    routeScope ??
    settings.data?.extensionScope ??
    (rememberedTool === null ? null : toolExtensionScope(rememberedTool));
  const { activeTab, activeScope, scopedTargets } = resolveExtensionScope(
    tools,
    desktopApps,
    displayedKind,
    displayedScope,
  );
  const activeKind = scopesReady ? activeTab.kind : null;

  const selectKind = (kind: ExtensionKind) => {
    if (save.isPending) return;
    if (kind === activeKind && failedKind === null) return;
    setLocalKind(kind);
    setRouteScope(null);
    setRouteKind(null);
    setFailedKind(null);
    save.mutate(
      { extensionKind: kind },
      {
        onSuccess: () => setLocalKind(null),
        onError: () => setFailedKind(kind),
      },
    );
  };

  const selectTarget = (scope: ExtensionScope) => {
    if (save.isPending) return;
    if (
      activeScope !== null &&
      extensionScopeKey(scope) === activeScope.key &&
      failedScope === null
    ) {
      return;
    }
    setLocalScope(scope);
    setRouteScope(null);
    setFailedScope(null);
    const patch: Partial<ProductSettings> = { extensionScope: scope };
    if (scope.kind === "tool") patch.toolScope = scope.id;
    save.mutate(patch, {
      onSuccess: () => setLocalScope(null),
      onError: () => setFailedScope(scope),
    });
  };

  const retry = () => {
    if (save.isPending || (failedKind === null && failedScope === null)) return;
    const retryKind = failedKind;
    const retryScope = failedScope;
    const patch: Partial<ProductSettings> = {};
    if (retryKind !== null) patch.extensionKind = retryKind;
    if (retryScope !== null) {
      patch.extensionScope = retryScope;
      if (retryScope.kind === "tool") patch.toolScope = retryScope.id;
    }
    save.mutate(patch, {
      onSuccess: () => {
        if (retryKind !== null) {
          setFailedKind(null);
          setLocalKind(null);
        }
        if (retryScope !== null) {
          setFailedScope(null);
          setLocalScope(null);
        }
      },
    });
  };

  return {
    activeKind,
    activeTab,
    activeScope,
    activeTool: activeScope?.tool ?? null,
    scopedTargets: activeKind === null ? [] : scopedTargets,
    saving: save.isPending,
    saveFailed: failedKind !== null || failedScope !== null,
    selectKind,
    selectTarget,
    retry,
  };
}
