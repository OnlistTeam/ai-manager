import { Plus } from "lucide-react";
import { useTranslation } from "react-i18next";
import { Button } from "@/shared/ui/Button";

interface ExtensionsPageActionsProps {
  canAddMcp: boolean;
  canAddPrompt: boolean;
  canAddSkill: boolean;
  mutationsBlocked: boolean;
  onAddMcp: () => void;
  onAddPrompt: () => void;
  onAddSkill: () => void;
}

export function ExtensionsPageActions({
  canAddMcp,
  canAddPrompt,
  canAddSkill,
  mutationsBlocked,
  onAddMcp,
  onAddPrompt,
  onAddSkill,
}: ExtensionsPageActionsProps) {
  const { t } = useTranslation();
  if (!canAddMcp && !canAddPrompt && !canAddSkill) return null;

  return (
    <div className="flex flex-wrap items-center gap-2">
      {canAddSkill ? (
        <Button size="sm" disabled={mutationsBlocked} onClick={onAddSkill}>
          <Plus className="h-4 w-4" aria-hidden="true" />
          {t("extensions.skill.catalog.add")}
        </Button>
      ) : null}
      {canAddMcp ? (
        <Button size="sm" disabled={mutationsBlocked} onClick={onAddMcp}>
          <Plus className="h-4 w-4" aria-hidden="true" />
          {t("extensions.mcp.install.add")}
        </Button>
      ) : null}
      {canAddPrompt ? (
        <Button size="sm" disabled={mutationsBlocked} onClick={onAddPrompt}>
          <Plus className="h-4 w-4" aria-hidden="true" />
          {t("extensions.prompt.editor.add")}
        </Button>
      ) : null}
    </div>
  );
}
