import {
  useMutation,
  useQueryClient,
  type UseMutationResult,
} from "@tanstack/react-query";
import { useTranslation } from "react-i18next";
import { toast } from "sonner";
import { operationKeys } from "@/entities/operation";
import { native, type ToolId } from "@/native";

export interface RemoveSkillVariables {
  tool: ToolId;
  skillId: string;
  name: string;
}

export function useRemoveSkill(): UseMutationResult<
  string,
  Error,
  RemoveSkillVariables
> {
  const queryClient = useQueryClient();
  const { t } = useTranslation();

  return useMutation({
    mutationFn: ({ tool, skillId }) => native.skills.remove(tool, skillId),
    onSuccess: (_operationId, { name }) => {
      toast.info(t("extensions.skill.remove.queued", { name }), {
        description: t("extensions.skill.remove.queuedDescription"),
      });
      return queryClient.invalidateQueries({ queryKey: operationKeys.all });
    },
  });
}
