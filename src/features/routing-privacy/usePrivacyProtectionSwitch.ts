import {
  usePrivacyProtection,
  useSetPrivacyProtection,
} from "@/entities/privacy-protection";
import type { PreferenceSaveState } from "@/shared/ui/PreferenceSaveStatus";

export interface PrivacyProtectionSwitchState {
  /** The stored choice; on while it is still loading, as the default is on. */
  enabled: boolean;
  /** No toggling while the value is unknown or a save is running. */
  disabled: boolean;
  loadFailed: boolean;
  retrying: boolean;
  saveState: PreferenceSaveState;
  toggle: (enabled: boolean) => void;
  retry: () => void;
}

export function usePrivacyProtectionSwitch(): PrivacyProtectionSwitchState {
  const query = usePrivacyProtection();
  const save = useSetPrivacyProtection();
  const saveState: PreferenceSaveState = save.isPending
    ? "saving"
    : save.isError
      ? "error"
      : save.isSuccess
        ? "saved"
        : "idle";

  return {
    enabled: query.data?.enabled ?? true,
    disabled: !query.data || save.isPending,
    loadFailed: query.isError && !query.data,
    retrying: query.isFetching,
    saveState,
    toggle: (enabled) => save.mutate(enabled),
    retry: () => void query.refetch(),
  };
}
