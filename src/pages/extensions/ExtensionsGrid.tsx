import { useTranslation } from "react-i18next";
import type {
  Extension,
  ExtensionKind,
  ExtensionScope,
} from "@/entities/extension";
import { extensionScopeKey } from "@/entities/extension";
import type { Operation } from "@/entities/operation";
import {
  ExtensionCard,
  useSetExtensionEnabled,
} from "@/features/extension-management";
import { ToolOperationProgress } from "@/features/tool-management";

export interface ExtensionsGridProps {
  scope: ExtensionScope;
  scopeName: string;
  kind: ExtensionKind;
  extensions: Extension[];
  activeOperation?: Operation;
  actionsBlocked: boolean;
  onEdit?: (extension: Extension) => void;
  onRemove?: (extension: Extension) => void;
}

/** One app's items as cards, for the kinds listed one app at a time. */
export function ExtensionsGrid({
  scope,
  scopeName,
  kind,
  extensions,
  activeOperation,
  actionsBlocked,
  onEdit,
  onRemove,
}: ExtensionsGridProps) {
  const { t } = useTranslation();
  const setEnabled = useSetExtensionEnabled();
  const failedVariables = setEnabled.isError ? setEnabled.variables : null;
  const failedError = setEnabled.isError ? setEnabled.error : null;
  const busy =
    actionsBlocked || setEnabled.isPending || activeOperation !== undefined;
  const targets = (extension: Extension, variables: typeof failedVariables) =>
    variables !== null &&
    variables !== undefined &&
    extensionScopeKey(variables.scope) === extensionScopeKey(scope) &&
    variables.kind === kind &&
    variables.extensionId === extension.id;

  return (
    <div className="flex flex-col gap-4">
      {activeOperation ? (
        <ToolOperationProgress
          operation={activeOperation}
          toolName={scopeName}
        />
      ) : null}

      <div className="grid gap-4">
        {extensions.map((extension, index) => {
          const operationPending =
            activeOperation?.extension?.id === extension.id;
          const togglePending =
            setEnabled.isPending && targets(extension, setEnabled.variables);
          const toggleFailed =
            failedError !== null && targets(extension, failedVariables);
          return (
            <ExtensionCard
              key={extension.id}
              extension={extension}
              position={index + 1}
              total={extensions.length}
              busy={busy}
              pending={togglePending || operationPending}
              pendingLabel={
                operationPending && activeOperation.messageKey
                  ? t(activeOperation.messageKey)
                  : undefined
              }
              failure={
                toggleFailed && failedVariables && failedError
                  ? {
                      error: failedError,
                      intendedEnabled: failedVariables.enabled,
                    }
                  : undefined
              }
              onEdit={
                onEdit && extension.management === "managed"
                  ? () => onEdit(extension)
                  : undefined
              }
              onRemove={
                onRemove &&
                extension.management === "managed" &&
                (extension.kind !== "prompt" || !extension.enabled)
                  ? () => onRemove(extension)
                  : undefined
              }
              onToggle={(enabled) => {
                if (toggleFailed && failedVariables) {
                  setEnabled.mutate(failedVariables);
                  return;
                }
                setEnabled.mutate({
                  scope,
                  kind,
                  extensionId: extension.id,
                  enabled,
                });
              }}
            />
          );
        })}
      </div>
    </div>
  );
}
