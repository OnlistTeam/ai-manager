import { useRef, useState } from "react";
import { useTranslation } from "react-i18next";
import { toast } from "sonner";
import type {
  ProviderCreateDraft,
  ProviderCreateResult,
  ProviderCustomCreateDraft,
  ProviderTestResult,
  ToolId,
} from "@/entities/provider";
import { generateUUID } from "@/utils/uuid";
import type { ProviderConnectTarget } from "./ProviderConnectModal";
import {
  useCreateCustomProvider,
  useCreateProvider,
  useRestoreToolLogin,
  useTestProvider,
} from "./useProviderMutations";

export interface ProviderConnectionRequest {
  tool: ToolId;
  toolName: string;
  draft: ProviderCreateDraft;
}

export interface ProviderCustomConnectionRequest {
  tool: ToolId;
  toolName: string;
  draft: ProviderCustomCreateDraft;
}

export interface ProviderCheckFailure {
  tool: ToolId;
  providerId: string;
  error: Error;
}

/**
 * Beginner connection orchestration: save first, then run the existing
 * reachability-only check against the exact record the backend created.
 *
 * The copy deliberately never calls this credential verification. The probe
 * only proves that the service address answered; the tool's first real request
 * remains the authority for API-key and model access.
 */
export interface UseProviderConnectionFlowOptions {
  onActivate?: (providerId: string) => void;
}

export function useProviderConnectionFlow({
  onActivate,
}: UseProviderConnectionFlowOptions = {}) {
  const { t } = useTranslation();
  const create = useCreateProvider();
  const createCustom = useCreateCustomProvider();
  const restoreLogin = useRestoreToolLogin();
  const test = useTestProvider({ notifyOnError: false });
  const [target, setTarget] = useState<ProviderConnectTarget | null>(null);
  const [loginOpen, setLoginOpen] = useState(false);
  const [checkFailure, setCheckFailure] = useState<ProviderCheckFailure | null>(
    null,
  );
  const requestIds = useRef(new Map<ToolId, string>());

  const openConnection = (next: ProviderConnectTarget) => {
    create.reset();
    createCustom.reset();
    setTarget(next);
  };
  const closeConnection = () => {
    create.reset();
    createCustom.reset();
    restoreLogin.reset();
    setTarget(null);
    setLoginOpen(false);
  };
  const openToolLogin = () => {
    restoreLogin.reset();
    setLoginOpen(true);
  };

  const checkProvider = (
    target: { tool: ToolId; providerId: string },
    onSuccess?: (result: ProviderTestResult) => void,
  ) => {
    setCheckFailure(null);
    test.mutate(target, {
      onSuccess,
      onError: (error) => setCheckFailure({ ...target, error }),
    });
  };

  const afterCreated =
    (tool: ToolId, toolName: string, draftName: string, onSaved?: () => void) =>
    (result: ProviderCreateResult) => {
      requestIds.current.delete(tool);
      setTarget(null);
      setLoginOpen(false);
      onSaved?.();
      const created = result.providers.find(
        (provider) => provider.id === result.createdProviderId,
      );
      const name = created?.name ?? draftName;
      const activateAction =
        created && !created.active && onActivate
          ? {
              label: t(
                created.additive
                  ? "ds.action.configure"
                  : "services.connect.activateNow",
              ),
              onClick: () => onActivate(created.id),
            }
          : undefined;

      if (!created?.testable) {
        toast.success(t("services.connect.saved", { name }), {
          description:
            created && !created.active
              ? t(
                  created.additive
                    ? "services.switch.chooseModelHint"
                    : "services.connect.activateQuestion",
                  { tool: toolName },
                )
              : t("services.connect.firstUse", { tool: toolName }),
          action: activateAction,
        });
        return;
      }

      checkProvider(
        { tool, providerId: result.createdProviderId },
        (outcome) => {
          const title = t(`services.connect.address.${outcome.reachability}`, {
            name,
          });
          const description = t(
            outcome.reachability === "failed"
              ? "services.connect.addressRetry"
              : "services.connect.firstUse",
            { tool: toolName },
          );
          if (outcome.reachability === "operational") {
            toast.success(title, { description, action: activateAction });
          } else {
            toast.info(title, { description, action: activateAction });
          }
        },
      );
    };

  const requestIdFor = (tool: ToolId) => {
    let requestId = requestIds.current.get(tool);
    if (!requestId) {
      requestId = generateUUID();
      requestIds.current.set(tool, requestId);
    }
    return requestId;
  };

  const connectProvider = (
    { tool, toolName, draft }: ProviderConnectionRequest,
    onSaved?: () => void,
  ) => {
    const requestId = requestIdFor(tool);
    create.mutate(
      { tool, requestId, draft },
      { onSuccess: afterCreated(tool, toolName, draft.name, onSaved) },
    );
  };

  const connectCustomProvider = (
    { tool, toolName, draft }: ProviderCustomConnectionRequest,
    onSaved?: () => void,
  ) => {
    const requestId = requestIdFor(tool);
    createCustom.mutate(
      { tool, requestId, draft },
      { onSuccess: afterCreated(tool, toolName, draft.name, onSaved) },
    );
  };

  /**
   * Adding the tool's own sign-in entry saves no key, so it takes the same
   * after-save path as any endpoint: a toast, and the offer to use it now.
   */
  const restoreToolLogin = (
    { tool, toolName }: { tool: ToolId; toolName: string },
    onSaved?: () => void,
  ) => {
    restoreLogin.mutate(
      { tool },
      { onSuccess: afterCreated(tool, toolName, toolName, onSaved) },
    );
  };

  /**
   * A sign-in from AI Manager finished (ADR-0061): the endpoint it added or
   * updated takes the after-save path of any other.
   */
  const signedIn = (
    { tool, toolName }: { tool: ToolId; toolName: string },
    result: ProviderCreateResult,
    onSaved?: () => void,
  ) => {
    restoreLogin.reset();
    afterCreated(tool, toolName, toolName, onSaved)(result);
  };

  const resetCreateError = () => {
    create.reset();
    createCustom.reset();
    restoreLogin.reset();
  };

  return {
    target,
    loginOpen,
    connectProvider,
    connectCustomProvider,
    restoreToolLogin,
    signedIn,
    checkProvider,
    checkFailure,
    checkBusy: test.isPending,
    createBusy:
      create.isPending || createCustom.isPending || restoreLogin.isPending,
    createError: create.error ?? createCustom.error ?? restoreLogin.error,
    openConnection,
    openToolLogin,
    closeConnection,
    resetCreateError,
    testingProviderId: test.isPending ? test.variables?.providerId : undefined,
  };
}
