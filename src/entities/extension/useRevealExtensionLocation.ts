import { useMutation } from "@tanstack/react-query";
import { native, type ExtensionKind, type ExtensionScope } from "@/native";

export function useRevealExtensionLocation() {
  return useMutation({
    mutationFn: ({
      scope,
      kind,
    }: {
      scope: ExtensionScope;
      kind: ExtensionKind;
    }) => native.extensions.revealLocation(scope, kind),
  });
}
