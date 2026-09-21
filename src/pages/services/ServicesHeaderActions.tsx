import { Plus } from "lucide-react";
import { useTranslation } from "react-i18next";
import { Button } from "@/shared/ui/Button";

interface ServicesHeaderActionsProps {
  canConnect: boolean;
  busy: boolean;
  actionsBlocked: boolean;
  connecting: boolean;
  onConnect: () => void;
}

export function ServicesHeaderActions({
  canConnect,
  busy,
  actionsBlocked,
  connecting,
  onConnect,
}: ServicesHeaderActionsProps) {
  const { t } = useTranslation();

  return (
    <div className="flex justify-end">
      <Button
        disabled={!canConnect || busy || actionsBlocked}
        loading={connecting}
        onClick={onConnect}
      >
        <Plus className="h-4 w-4" aria-hidden="true" />
        {t("services.action.add")}
      </Button>
    </div>
  );
}
