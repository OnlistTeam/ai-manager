import { useTranslation } from "react-i18next";
import type { Tool, ToolId } from "@/entities/tool";
import { ToolScopeTabs } from "@/features/provider-management";
import { ScopeSaveNotice } from "@/features/scope-memory";

export interface ServicesScopePickerProps {
  tools: readonly Tool[];
  active: ToolId | null;
  activeName: string;
  disabled: boolean;
  saving: boolean;
  saveFailed: boolean;
  onSelect: (id: ToolId) => void;
  onRetry: () => void;
}

export function ServicesScopePicker({
  tools,
  active,
  activeName,
  disabled,
  saving,
  saveFailed,
  onSelect,
  onRetry,
}: ServicesScopePickerProps) {
  const { t } = useTranslation();

  return (
    <div className="flex flex-col gap-3">
      <ToolScopeTabs
        tools={tools}
        active={active}
        disabled={disabled}
        label={t("services.scope")}
        unsupportedLabel={t("services.scopeUnsupportedShort")}
        onSelect={onSelect}
      />

      <ScopeSaveNotice
        saving={saving}
        failed={saveFailed}
        savingLabel={t("services.scopeSave.saving")}
        errorTitle={t("services.scopeSave.errorTitle")}
        errorDescription={t("services.scopeSave.errorDescription", {
          tool: activeName,
        })}
        retryLabel={t("services.scopeSave.retry")}
        retryDisabled={disabled}
        onRetry={onRetry}
      />
    </div>
  );
}
