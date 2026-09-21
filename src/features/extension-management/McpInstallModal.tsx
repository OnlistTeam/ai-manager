import { AlertCircle } from "lucide-react";
import { useEffect, useRef, useState, type FormEvent } from "react";
import { useTranslation } from "react-i18next";
import type { ExtensionScope } from "@/entities/extension";
import { toErrorCopy } from "@/shared/lib/nativeError";
import { Button } from "@/shared/ui/Button";
import { Modal } from "@/shared/ui/Modal";
import { ExtensionMutationPausedNotice } from "./ExtensionMutationPausedNotice";
import {
  EMPTY_MCP_INSTALL_VALUES,
  mcpDraftFrom,
  validateMcpInstall,
  type McpInstallErrors,
  type McpInstallValues,
  type McpTransport,
} from "./mcpInstallForm";
import { McpInstallFields } from "./McpInstallFields";
import { useInstallMcp } from "./useMcpInstallation";

const FORM_ID = "mcp-install-form";

export interface McpInstallModalProps {
  open: boolean;
  scope: ExtensionScope;
  scopeName: string;
  mutationsBlocked: boolean;
  onOpenChange: (open: boolean) => void;
}

export function McpInstallModal({
  open,
  scope,
  scopeName,
  mutationsBlocked,
  onOpenChange,
}: McpInstallModalProps) {
  const { t } = useTranslation();
  const [values, setValues] = useState<McpInstallValues>(
    EMPTY_MCP_INSTALL_VALUES,
  );
  const [errors, setErrors] = useState<McpInstallErrors>({});
  const nameRef = useRef<HTMLInputElement>(null);
  const commandRef = useRef<HTMLInputElement>(null);
  const urlRef = useRef<HTMLInputElement>(null);
  const install = useInstallMcp();
  const resetInstall = install.reset;
  const backendError = install.error ? toErrorCopy(install.error) : null;

  useEffect(() => {
    if (!open) return;
    setValues(EMPTY_MCP_INSTALL_VALUES);
    setErrors({});
    resetInstall();
  }, [open, resetInstall, scope]);

  const clearError = (field: keyof McpInstallErrors) => {
    setErrors((current) => ({ ...current, [field]: undefined }));
    if (install.error) resetInstall();
  };
  const changeField = (field: keyof McpInstallValues, value: string) => {
    setValues((current) => ({ ...current, [field]: value }));
    clearError(field as keyof McpInstallErrors);
  };
  const chooseTransport = (transport: McpTransport) => {
    setValues((current) => ({ ...current, transport }));
    setErrors({});
    if (install.error) resetInstall();
  };
  const submit = (event: FormEvent<HTMLFormElement>) => {
    event.preventDefault();
    if (mutationsBlocked) return;
    const nextErrors = validateMcpInstall(values);
    setErrors(nextErrors);
    if (Object.keys(nextErrors).length > 0) {
      if (nextErrors.name) nameRef.current?.focus();
      else if (nextErrors.command) commandRef.current?.focus();
      else if (nextErrors.url) urlRef.current?.focus();
      return;
    }
    install.mutate(
      { scope, draft: mcpDraftFrom(values) },
      { onSuccess: () => onOpenChange(false) },
    );
  };

  return (
    <Modal
      open={open}
      size="lg"
      dismissible={!install.isPending}
      initialFocusRef={nameRef}
      onOpenChange={onOpenChange}
      title={t("extensions.mcp.install.title")}
      description={t("extensions.mcp.install.description", {
        tool: scopeName,
      })}
      footer={
        <>
          <Button
            variant="secondary"
            disabled={install.isPending}
            onClick={() => onOpenChange(false)}
          >
            {t("ds.action.cancel")}
          </Button>
          <Button
            type="submit"
            form={FORM_ID}
            disabled={mutationsBlocked}
            loading={install.isPending}
          >
            {t("extensions.mcp.install.confirm")}
          </Button>
        </>
      }
    >
      {mutationsBlocked ? <ExtensionMutationPausedNotice /> : null}

      <form
        id={FORM_ID}
        className="scrollbar-subtle flex max-h-[56vh] flex-col gap-4 overflow-y-auto pr-1"
        onSubmit={submit}
      >
        <McpInstallFields
          values={values}
          errors={errors}
          disabled={install.isPending}
          nameRef={nameRef}
          commandRef={commandRef}
          urlRef={urlRef}
          onFieldChange={changeField}
          onTransportChange={chooseTransport}
        />

        {backendError ? (
          <div
            role="alert"
            className="flex items-start gap-2 rounded-md border border-danger/30 bg-danger/5 p-3"
          >
            <AlertCircle
              className="mt-0.5 h-4 w-4 shrink-0 text-danger"
              aria-hidden="true"
            />
            <div>
              <p className="text-caption font-medium text-content">
                {t(backendError.messageKey)}
              </p>
              {backendError.remediationKey ? (
                <p className="mt-0.5 text-caption text-content-muted">
                  {t(backendError.remediationKey)}
                </p>
              ) : null}
            </div>
          </div>
        ) : null}
      </form>
    </Modal>
  );
}
