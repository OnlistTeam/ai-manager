import { useTranslation } from "react-i18next";
import { ConfirmActionModal } from "@/features/tool-management";
import type { Tool } from "@/entities/tool";

export type DirectQuickCheckResolution = "updateTool" | "repairTool";

export type QuickCheckConfirmation =
  | { resolution: "updateTool"; tool: Tool }
  | { resolution: "repairTool"; tool: Tool };

type QuickCheckRepairConfirmation = Extract<
  QuickCheckConfirmation,
  { resolution: "repairTool" }
>;

interface QuickCheckResolutionModalProps {
  confirmation: QuickCheckRepairConfirmation | null;
  busy: boolean;
  disabled: boolean;
  error: Error | null;
  onClose: () => void;
  onConfirm: () => void;
}

export function QuickCheckResolutionModal({
  confirmation,
  busy,
  disabled,
  error,
  onClose,
  onConfirm,
}: QuickCheckResolutionModalProps) {
  const { t } = useTranslation();

  return (
    <ConfirmActionModal
      open={confirmation !== null}
      onOpenChange={(open) => {
        if (!open) onClose();
      }}
      title={t("tools.confirm.repair.title", {
        name: confirmation?.tool.name ?? "",
      })}
      description={t("tools.confirm.repair.body")}
      confirmLabel={t("tools.confirm.repair.confirm")}
      busy={busy}
      confirmDisabled={disabled}
      error={error}
      onConfirm={onConfirm}
    />
  );
}
