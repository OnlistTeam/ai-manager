import { useMutation, type UseMutationResult } from "@tanstack/react-query";
import { native } from "@/native";

/**
 * Opens the published source repository. AGPL-3.0 §5 requires the running
 * program to say where the corresponding source is; the address itself lives
 * on the native side.
 */
export function useOpenSourceCode(): UseMutationResult<boolean, Error, void> {
  return useMutation<boolean, Error, void>({
    mutationFn: () => native.about.openSourceCode(),
  });
}

/** Opens the licence text in the browser instead of embedding it in the app. */
export function useOpenLicense(): UseMutationResult<boolean, Error, void> {
  return useMutation<boolean, Error, void>({
    mutationFn: () => native.about.openLicense(),
  });
}
