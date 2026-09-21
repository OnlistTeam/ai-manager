import { AlertCircle, AlertTriangle } from "lucide-react";
import { useEffect, useRef } from "react";
import { useTranslation } from "react-i18next";
import type { Extension } from "@/entities/extension";
import { toErrorCopy } from "@/shared/lib/nativeError";
import { Button } from "@/shared/ui/Button";
import { Modal } from "@/shared/ui/Modal";
import { ExtensionMutationPausedNotice } from "./ExtensionMutationPausedNotice";
import { useRemoveMcp } from "./useMcpRemoval";

const IMPACT_KEYS = [
  "extensions.mcp.remove.point.everywhere",
  "extensions.mcp.remove.point.keepsTargets",
  "extensions.mcp.remove.point.noUndo",
] as const;

export interface McpRemovalModalProps {
  connection: Extension | null;
  mutationsBlocked: boolean;
  onOpenChange: (open: boolean) => void;
}

export function McpRemovalModal({
  connection,
  mutationsBlocked,
  onOpenChange,
}: McpRemovalModalProps) {
  const { t } = useTranslation();
  const cancelRef = useRef<HTMLButtonElement>(null);
  const remove = useRemoveMcp();
  const resetRemoval = remove.reset;
  const error = remove.error ? toErrorCopy(remove.error) : null;

  useEffect(() => {
    resetRemoval();
  }, [connection?.id, resetRemoval]);

  return (
    <Modal
      open={connection !== null}
      size="md"
      dismissible={!remove.isPending}
      initialFocusRef={cancelRef}
      onOpenChange={onOpenChange}
      title={t("extensions.mcp.remove.title", {
        name: connection?.name ?? "",
      })}
      description={t("extensions.mcp.remove.description")}
      footer={
        <>
          <Button
            ref={cancelRef}
            variant="secondary"
            disabled={remove.isPending}
            onClick={() => onOpenChange(false)}
          >
            {t("ds.action.cancel")}
          </Button>
          <Button
            variant="danger"
            disabled={mutationsBlocked}
            loading={remove.isPending}
            onClick={() => {
              if (!connection || mutationsBlocked) return;
              remove.mutate(
                {
                  scope: connection.scope,
                  mcpId: connection.id,
                  name: connection.name,
                },
                { onSuccess: () => onOpenChange(false) },
              );
            }}
          >
            {t("extensions.mcp.remove.confirm")}
          </Button>
        </>
      }
    >
      {mutationsBlocked ? <ExtensionMutationPausedNotice /> : null}

      <ul className="flex flex-col gap-2 rounded-md bg-danger/10 p-3">
        {IMPACT_KEYS.map((key) => (
          <li
            key={key}
            className="flex items-start gap-2 text-caption text-content"
          >
            <AlertTriangle
              className="mt-0.5 h-4 w-4 shrink-0 text-danger"
              aria-hidden="true"
            />
            {t(key)}
          </li>
        ))}
      </ul>

      {error ? (
        <div
          role="alert"
          className="mt-3 flex items-start gap-2 rounded-md border border-danger/30 bg-danger/5 p-3"
        >
          <AlertCircle
            className="mt-0.5 h-4 w-4 shrink-0 text-danger"
            aria-hidden="true"
          />
          <div>
            <p className="text-caption font-medium text-content">
              {t(error.messageKey)}
            </p>
            <p className="mt-0.5 text-caption leading-5 text-content-muted">
              {t("extensions.mcp.remove.errorRetry")}
            </p>
          </div>
        </div>
      ) : null}
    </Modal>
  );
}
