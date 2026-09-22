import { FileCog, FolderOpen } from "lucide-react";
import { useTranslation } from "react-i18next";
import {
  useExtensionLocation,
  useOpenExtensionLocation,
  type ExtensionKind,
  type ExtensionScope,
} from "@/entities/extension";
import { toErrorCopy } from "@/shared/lib/nativeError";
import { Button } from "@/shared/ui/Button";
import { CopyButton } from "@/shared/ui/CopyButton";

/**
 * The file behind the list above it.
 *
 * MCP servers and global instructions are not one file each: every entry for a
 * tool is written into the same JSON or TOML that tool reads. So this belongs
 * to the list, not to a card — repeating the identical path under each row
 * would be the same sentence N times.
 *
 * Skills are the other shape entirely, one directory per entry, so they get
 * their actions on the card and no row here (ADR-0045).
 */
export function ExtensionLocationRow({
  scope,
  kind,
}: {
  scope: ExtensionScope;
  kind: ExtensionKind;
}) {
  const { t } = useTranslation();
  const location = useExtensionLocation(scope, kind);
  const open = useOpenExtensionLocation();
  const error = open.error ? toErrorCopy(open.error) : null;

  if (!location.data) return null;
  const { path, exists } = location.data;

  return (
    <div className="flex min-w-0 flex-col gap-1.5 rounded-lg border border-hairline bg-layer-1 px-4 py-3">
      <div className="flex min-w-0 flex-wrap items-center gap-x-3 gap-y-2">
        <span className="text-caption text-content-muted">
          {t("extensions.location.label")}
        </span>
        <span className="min-w-0 flex-1 break-all font-mono text-mono-sm text-content">
          {path}
        </span>
        <CopyButton value={path} label={t("extensions.location.label")} />
        <Button
          variant="ghost"
          size="sm"
          disabled={!exists || open.isPending}
          loading={open.isPending && open.variables?.action === "edit"}
          onClick={() => open.mutate({ scope, kind, action: "edit" })}
        >
          <FileCog className="h-4 w-4" aria-hidden="true" />
          {t("extensions.location.open")}
        </Button>
        <Button
          variant="ghost"
          size="sm"
          disabled={open.isPending}
          loading={open.isPending && open.variables?.action === "browse"}
          onClick={() => open.mutate({ scope, kind, action: "browse" })}
        >
          <FolderOpen className="h-4 w-4" aria-hidden="true" />
          {t("extensions.location.action")}
        </Button>
      </div>
      {/* A tool that has not written its config yet is the normal first-run
          state, so this says so rather than letting the disabled button look
          like a fault. */}
      {exists ? null : (
        <p className="text-caption text-content-muted">
          {t("extensions.location.notYetWritten")}
        </p>
      )}
      {error ? (
        <p role="alert" className="text-caption text-danger">
          {t(error.messageKey)}{" "}
          {error.remediationKey ? t(error.remediationKey) : null}
        </p>
      ) : null}
    </div>
  );
}
