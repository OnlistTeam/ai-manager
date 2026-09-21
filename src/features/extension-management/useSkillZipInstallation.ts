import {
  useMutation,
  useQueryClient,
  type UseMutationResult,
} from "@tanstack/react-query";
import { useTranslation } from "react-i18next";
import { toast } from "sonner";
import { operationKeys } from "@/entities/operation";
import { native, type SkillZipInstallOutcome, type ToolId } from "@/native";

export function useInstallSkillZip(): UseMutationResult<
  SkillZipInstallOutcome,
  Error,
  ToolId
> {
  const queryClient = useQueryClient();
  const { t } = useTranslation();

  return useMutation({
    mutationFn: (tool) => native.skills.installZip(tool),
    onSuccess: (outcome) => {
      if (outcome.status === "cancelled") return;
      void queryClient.invalidateQueries({ queryKey: operationKeys.all });
      toast.info(t("extensions.skill.zip.queued"), {
        description: t("extensions.skill.zip.queuedDescription"),
      });
    },
  });
}
