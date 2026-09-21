import { FileCog, FolderOpen } from "lucide-react";
import { useTranslation } from "react-i18next";
import { toast } from "sonner";
import {
  useOpenProviderRuntimeResource,
  type ProviderRuntimeResource,
} from "@/entities/provider";
import type { ToolId } from "@/entities/tool";
import { toErrorCopy } from "@/shared/lib/nativeError";
import { Button } from "@/shared/ui/Button";

export interface ServicesOpenConfigActionProps {
  tool: ToolId;
  /** The tool's live configuration resource, if the backend reports one. */
  resource: ProviderRuntimeResource | undefined;
  actionsBlocked?: boolean;
}

/**
 * Opens the config this tool actually reads. The backend decides the action
 * based on whether the file exists, so the button's wording follows
 * `action`: "open file" if it exists, "open folder" if it doesn't yet — never
 * promising an action a click can't deliver.
 *
 * Only shown on the card that's currently in use: every other endpoint only
 * exists in the database, with no file belonging to it.
 */
export function ServicesOpenConfigAction({
  tool,
  resource,
  actionsBlocked = false,
}: ServicesOpenConfigActionProps) {
  const { t } = useTranslation();
  const openResource = useOpenProviderRuntimeResource();

  if (!resource) return null;

  const editable = resource.action === "edit";
  const Icon = editable ? FileCog : FolderOpen;

  return (
    <Button
      variant="ghost"
      disabled={actionsBlocked}
      loading={openResource.isPending}
      onClick={() => {
        openResource.reset();
        openResource.mutate(
          { tool, resource: resource.id },
          {
            onError: (openError) => {
              const copy = toErrorCopy(openError);
              toast.error(t(copy.messageKey), {
                description: copy.remediationKey
                  ? t(copy.remediationKey)
                  : undefined,
              });
            },
          },
        );
      }}
    >
      <Icon className="h-4 w-4" aria-hidden="true" />
      {t(
        editable
          ? "services.runtime.openConfigFile"
          : "services.runtime.openConfigFolder",
      )}
    </Button>
  );
}
