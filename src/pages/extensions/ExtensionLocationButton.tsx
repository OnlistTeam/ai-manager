import { FolderOpen } from "lucide-react";
import { useTranslation } from "react-i18next";
import {
  useRevealExtensionLocation,
  type ExtensionKind,
  type ExtensionScope,
} from "@/entities/extension";
import { toErrorCopy } from "@/shared/lib/nativeError";
import { Button } from "@/shared/ui/Button";

/** Opens the tool's live file, not a particular saved prompt/MCP database row. */
export function ExtensionLocationButton({
  scope,
  kind,
}: {
  scope: ExtensionScope;
  kind: ExtensionKind;
}) {
  const { t } = useTranslation();
  const reveal = useRevealExtensionLocation();
  const error = reveal.error ? toErrorCopy(reveal.error) : null;
  return (
    <div className="flex flex-col items-start gap-2">
      <Button
        variant="ghost"
        size="sm"
        loading={reveal.isPending}
        title={t("extensions.location.description")}
        onClick={() => reveal.mutate({ scope, kind })}
      >
        <FolderOpen className="h-4 w-4" aria-hidden="true" />
        {t("extensions.location.action")}
      </Button>
      {error ? (
        <p role="alert" className="text-caption text-danger">
          {t(error.messageKey)}{" "}
          {error.remediationKey ? t(error.remediationKey) : null}
        </p>
      ) : null}
    </div>
  );
}
