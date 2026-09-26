import { useState } from "react";
import {
  usePrivacyProtection,
  useSetPrivacyProtection,
  type PrivacyProtection,
  type PrivacyProtectionPatch,
} from "@/entities/privacy-protection";
import { toErrorCopy } from "@/shared/lib/nativeError";
import type { PreferenceSaveState } from "@/shared/ui/PreferenceSaveStatus";

export type PrivacyField = keyof PrivacyProtection;

export interface PrivacySettingsState {
  data: PrivacyProtection | undefined;
  loading: boolean;
  retrying: boolean;
  saving: boolean;
  /** The save status of one row; only the row last changed shows one. */
  stateFor: (field: PrivacyField) => PreferenceSaveState;
  /** The backend's message for a failed save of `field`, if any. */
  errorKeyFor: (field: PrivacyField) => string | null;
  save: (field: PrivacyField, patch: PrivacyProtectionPatch) => void;
  retry: () => void;
}

export function usePrivacySettings(): PrivacySettingsState {
  const query = usePrivacyProtection();
  const mutation = useSetPrivacyProtection();
  const [target, setTarget] = useState<PrivacyField | null>(null);
  const current: PreferenceSaveState = mutation.isPending
    ? "saving"
    : mutation.isError
      ? "error"
      : mutation.isSuccess
        ? "saved"
        : "idle";

  return {
    data: query.data,
    loading: query.isPending,
    retrying: query.isFetching,
    saving: mutation.isPending,
    stateFor: (field) => (target === field ? current : "idle"),
    errorKeyFor: (field) =>
      target === field && mutation.error
        ? toErrorCopy(mutation.error).messageKey
        : null,
    save: (field, patch) => {
      setTarget(field);
      mutation.mutate(patch);
    },
    retry: () => void query.refetch(),
  };
}
