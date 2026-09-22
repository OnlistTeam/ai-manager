import { useMutation } from "@tanstack/react-query";
import { ExternalLink } from "lucide-react";
import { useTranslation } from "react-i18next";
import { native } from "@/native";
import { Button } from "@/shared/ui/Button";

/**
 * The remediation key that has a button behind it.
 *
 * macOS asks for permission to control Terminal exactly once. After "Don't
 * Allow" there is no second prompt, so prose telling the user to visit a
 * four-level-deep settings pane is not a remedy — the button is.
 */
export const AUTOMATION_REMEDIATION_KEY =
  "error.remediation.allowTerminalAutomation";

/** Opens the pane where the refused grant can be switched back on. */
export function AutomationGrantAction() {
  const { t } = useTranslation();
  const open = useMutation({
    mutationFn: () => native.system.openAutomationSettings(),
  });

  return (
    <Button
      size="sm"
      variant="secondary"
      className="mt-2"
      disabled={open.isPending}
      onClick={() => open.mutate()}
    >
      <ExternalLink className="h-4 w-4" aria-hidden="true" />
      {t("ds.action.openSystemSettings")}
    </Button>
  );
}
