import { useEffect, useRef, useState, type FormEvent } from "react";
import { useTranslation } from "react-i18next";
import type { ExtensionScope } from "@/entities/extension";
import { toErrorCopy } from "@/shared/lib/nativeError";
import { Button } from "@/shared/ui/Button";
import { Modal } from "@/shared/ui/Modal";
import { ExtensionMutationPausedNotice } from "./ExtensionMutationPausedNotice";
import type { McpPastedConnection } from "./mcpConfigPaste";
import { McpConfigPastePanel } from "./McpConfigPastePanel";
import { McpFormAlert } from "./McpFormAlert";
import {
  EMPTY_MCP_INSTALL_VALUES,
  mcpDraftFrom,
  mcpValuesFrom,
  validateMcpInstall,
  type McpInstallErrors,
  type McpInstallValues,
  type McpTextField,
  type McpTransport,
  type McpVariableKind,
  type McpVariableRow,
} from "./mcpInstallForm";
import { McpInstallFields } from "./McpInstallFields";
import { useInstallMcp } from "./useMcpInstallation";
import { useMcpEditForm, useUpdateMcp, type McpEditTarget } from "./useMcpEdit";

const FORM_ID = "mcp-install-form";

export interface McpInstallModalProps {
  open: boolean;
  scope: ExtensionScope;
  scopeName: string;
  mutationsBlocked: boolean;
  /**
   * A saved connection to edit in place (ADR-0062). The form is prefilled
   * from it, its id stays fixed, and saving keeps every app's switch.
   */
  editing?: McpEditTarget | null;
  onOpenChange: (open: boolean) => void;
}

export function McpInstallModal({
  open,
  scope,
  scopeName,
  mutationsBlocked,
  editing = null,
  onOpenChange,
}: McpInstallModalProps) {
  const { t } = useTranslation();
  const [values, setValues] = useState<McpInstallValues>(
    EMPTY_MCP_INSTALL_VALUES,
  );
  const [errors, setErrors] = useState<McpInstallErrors>({});
  const hydratedId = useRef<string | null>(null);
  const nameRef = useRef<HTMLInputElement>(null);
  const commandRef = useRef<HTMLInputElement>(null);
  const urlRef = useRef<HTMLInputElement>(null);
  const install = useInstallMcp();
  const update = useUpdateMcp();
  const saved = useMcpEditForm(editing, open);
  const write = editing ? update : install;
  const resetWrite = write.reset;
  const reading = editing !== null && saved.isPending;
  const readFailed = editing !== null && saved.isError;
  const failure = readFailed ? saved.error : write.error;
  const failureCopy = failure ? toErrorCopy(failure) : null;
  const busy = write.isPending || reading;

  useEffect(() => {
    if (!open) return;
    hydratedId.current = null;
    setValues(EMPTY_MCP_INSTALL_VALUES);
    setErrors({});
    resetWrite();
  }, [open, resetWrite, scope, editing?.id]);

  useEffect(() => {
    if (!open || !saved.data || hydratedId.current === saved.data.id) return;
    hydratedId.current = saved.data.id;
    setValues(mcpValuesFrom(saved.data));
  }, [open, saved.data]);

  const clearError = (field: keyof McpInstallErrors) => {
    setErrors((current) => ({ ...current, [field]: undefined }));
    if (write.error) resetWrite();
  };
  const changeField = (field: McpTextField, value: string) => {
    setValues((current) => ({ ...current, [field]: value }));
    clearError(field);
  };
  const changeVariables = (kind: McpVariableKind, rows: McpVariableRow[]) => {
    setValues((current) => ({ ...current, [kind]: rows }));
    clearError(kind);
  };
  const fillFromPaste = ({ name, fields }: McpPastedConnection) => {
    setValues((current) => ({
      ...current,
      ...fields,
      name: name ?? current.name,
    }));
    setErrors({});
    if (write.error) resetWrite();
    nameRef.current?.focus();
  };
  const chooseTransport = (transport: McpTransport) => {
    setValues((current) => ({ ...current, transport }));
    setErrors({});
    if (write.error) resetWrite();
  };
  const submit = (event: FormEvent<HTMLFormElement>) => {
    event.preventDefault();
    if (mutationsBlocked || reading || readFailed) return;
    const nextErrors = validateMcpInstall(values);
    setErrors(nextErrors);
    if (Object.keys(nextErrors).length > 0) {
      if (nextErrors.name) nameRef.current?.focus();
      else if (nextErrors.command) commandRef.current?.focus();
      else if (nextErrors.url) urlRef.current?.focus();
      return;
    }
    const draft = mcpDraftFrom(values);
    const close = { onSuccess: () => onOpenChange(false) };
    if (editing) update.mutate({ target: editing, draft }, close);
    else install.mutate({ scope, draft }, close);
  };

  return (
    <Modal
      open={open}
      size="lg"
      dismissible={!write.isPending}
      initialFocusRef={nameRef}
      onOpenChange={onOpenChange}
      title={
        editing
          ? t("extensions.mcp.edit.title", { name: editing.name })
          : t("extensions.mcp.install.title")
      }
      description={
        editing
          ? t(
              editing.found.length > 0
                ? "extensions.mcp.edit.descriptionFound"
                : "extensions.mcp.edit.description",
            )
          : t("extensions.mcp.install.description", { tool: scopeName })
      }
      footer={
        <>
          <Button
            variant="secondary"
            disabled={write.isPending}
            onClick={() => onOpenChange(false)}
          >
            {t("ds.action.cancel")}
          </Button>
          <Button
            type="submit"
            form={FORM_ID}
            disabled={mutationsBlocked || reading || readFailed}
            loading={write.isPending}
          >
            {editing
              ? t("extensions.mcp.edit.confirm")
              : t("extensions.mcp.install.confirm")}
          </Button>
        </>
      }
    >
      {mutationsBlocked ? <ExtensionMutationPausedNotice /> : null}

      <form
        id={FORM_ID}
        aria-busy={reading || undefined}
        className="scrollbar-subtle flex max-h-[56vh] flex-col gap-4 overflow-y-auto pr-1"
        onSubmit={submit}
      >
        {editing ? null : (
          <McpConfigPastePanel disabled={busy} onApply={fillFromPaste} />
        )}

        {reading ? (
          <p role="status" className="text-caption text-content-muted">
            {t("extensions.mcp.edit.loading")}
          </p>
        ) : null}

        <McpInstallFields
          values={values}
          errors={errors}
          disabled={busy || readFailed}
          nameRef={nameRef}
          commandRef={commandRef}
          urlRef={urlRef}
          onFieldChange={changeField}
          onVariablesChange={changeVariables}
          onTransportChange={chooseTransport}
        />

        {failureCopy ? (
          <McpFormAlert
            messageKey={failureCopy.messageKey}
            remediationKey={failureCopy.remediationKey}
          />
        ) : null}
      </form>
    </Modal>
  );
}
