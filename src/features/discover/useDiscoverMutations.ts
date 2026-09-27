import { useMutation } from "@tanstack/react-query";
import { useTranslation } from "react-i18next";
import { toast } from "sonner";
import type { DiscoverLink } from "@/entities/discover";
import {
  native,
  type DiscoverMcpInstall,
  type ExtensionKind,
  type ToolId,
} from "@/native";
import { toErrorCopy } from "@/shared/lib/nativeError";

export function useInstallDiscoveredMcp() {
  return useMutation({
    mutationFn: (install: DiscoverMcpInstall) =>
      native.discover.installMcp(install),
  });
}

export function useInstallDiscoveredSkill() {
  return useMutation({
    mutationFn: ({ skill, tools }: { skill: string; tools: ToolId[] }) =>
      native.discover.installSkill(skill, tools),
  });
}

interface OpenLinkVariables {
  kind: ExtensionKind;
  id: string;
  link: DiscoverLink;
}

/** Opens one of an item's pages in the browser; native owns the address. */
export function useOpenDiscoverLink() {
  const { t } = useTranslation();
  return useMutation({
    mutationFn: ({ kind, id, link }: OpenLinkVariables) =>
      native.discover.openLink(kind, id, link),
    onError: (error) => {
      toast.error(t(toErrorCopy(error).messageKey));
    },
  });
}
