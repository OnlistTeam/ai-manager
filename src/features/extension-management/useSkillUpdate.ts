import {
  useMutation,
  useQueryClient,
  type UseMutationResult,
} from "@tanstack/react-query";
import { useTranslation } from "react-i18next";
import { toast } from "sonner";
import { operationKeys } from "@/entities/operation";
import { native, type ToolId } from "@/native";

export interface UpdateSkillVariables {
  tool: ToolId;
  skillId: string;
  name: string;
}

export function useUpdateSkill(): UseMutationResult<
  string,
  Error,
  UpdateSkillVariables
> {
  const queryClient = useQueryClient();
  const { t } = useTranslation();

  return useMutation({
    mutationFn: ({ tool, skillId }) => native.skills.update(tool, skillId),
    onSuccess: (_operationId, { name }) => {
      toast.info(t("extensions.skill.update.queued", { name }), {
        description: t("extensions.skill.update.queuedDescription"),
      });
      return queryClient.invalidateQueries({ queryKey: operationKeys.all });
    },
  });
}
