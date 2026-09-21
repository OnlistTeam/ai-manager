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
import {
  useCreateCustomProvider,
  useCreateProvider,
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
  const test = useTestProvider({ notifyOnError: false });
  const [connecting, setConnecting] = useState(false);
  const [preferCompatible, setPreferCompatible] = useState(false);
  const [checkFailure, setCheckFailure] = useState<ProviderCheckFailure | null>(
    null,
  );
  const requestIds = useRef(new Map<ToolId, string>());

  const openConnection = () => {
    create.reset();
    createCustom.reset();
    setPreferCompatible(false);
    setConnecting(true);
  };
  const openCompatibleConnection = () => {
    create.reset();
    createCustom.reset();
    setPreferCompatible(true);
    setConnecting(true);
  };
  const closeConnection = () => {
    create.reset();
    createCustom.reset();
    setConnecting(false);
  };
  const setConnectionOpen = (open: boolean) => {
    if (open) openConnection();
    else closeConnection();
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
      setConnecting(false);
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

  const resetCreateError = () => {
    create.reset();
    createCustom.reset();
  };

  return {
    connecting,
    preferCompatible,
    connectProvider,
    connectCustomProvider,
    checkProvider,
    checkFailure,
    checkBusy: test.isPending,
    createBusy: create.isPending || createCustom.isPending,
    createError: create.error ?? createCustom.error,
    openConnection,
    openCompatibleConnection,
    closeConnection,
    setConnectionOpen,
    resetCreateError,
    testingProviderId: test.isPending ? test.variables?.providerId : undefined,
  };
}
