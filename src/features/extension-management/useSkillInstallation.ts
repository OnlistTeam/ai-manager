import {
  useMutation,
  useQueryClient,
  type UseMutationResult,
} from "@tanstack/react-query";
import { useTranslation } from "react-i18next";
import { toast } from "sonner";
import { operationKeys } from "@/entities/operation";
import { native, type SkillCatalogItem, type ToolId } from "@/native";

export interface InstallSkillVariables {
  tool: ToolId;
  skill: SkillCatalogItem;
}

export function useInstallSkill(): UseMutationResult<
  string,
  Error,
  InstallSkillVariables
> {
  const queryClient = useQueryClient();
  const { t } = useTranslation();

  return useMutation({
    mutationFn: ({ tool, skill }) => native.skills.install(tool, skill),
    onSuccess: (_operationId, { skill }) => {
      void queryClient.invalidateQueries({ queryKey: operationKeys.all });
      toast.info(t("extensions.skill.catalog.queued", { name: skill.name }), {
        description: t("extensions.skill.catalog.queuedDescription"),
      });
    },
  });
}
