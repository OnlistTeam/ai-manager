import {
  useMutation,
  useQueryClient,
  type UseMutationResult,
} from "@tanstack/react-query";
import { useTranslation } from "react-i18next";
import { toast } from "sonner";
import { operationKeys } from "@/entities/operation";
import { skillBackupKeys, type SkillBackup } from "@/entities/skill-backup";
import { native, type ToolId } from "@/native";

export interface RestoreSkillBackupVariables {
  tool: ToolId;
  backup: SkillBackup;
}

export function useRestoreSkillBackup(): UseMutationResult<
  string,
  Error,
  RestoreSkillBackupVariables
> {
  const queryClient = useQueryClient();
  const { t } = useTranslation();

  return useMutation({
    mutationFn: ({ tool, backup }) =>
      native.skills.restoreBackup(tool, backup.id),
    onSuccess: (_operation, { backup }) => {
      void queryClient.invalidateQueries({ queryKey: operationKeys.all });
      toast.info(
        t("extensions.skill.backups.restoreQueued", { name: backup.name }),
        {
          description: t("extensions.skill.backups.restoreQueuedDescription"),
        },
      );
    },
  });
}

export function useDeleteSkillBackup(): UseMutationResult<
  SkillBackup[],
  Error,
  SkillBackup
> {
  const queryClient = useQueryClient();
  const { t } = useTranslation();

  return useMutation({
    mutationFn: (backup) => native.skills.deleteBackup(backup.id),
    onSuccess: (backups, deleted) => {
      queryClient.setQueryData(skillBackupKeys.list(), backups);
      toast.success(
        t("extensions.skill.backups.deleteSuccess", { name: deleted.name }),
      );
    },
    onSettled: async () => {
      // remove_dir_all can make partial progress before an I/O failure. Always
      // re-read the authoritative directory instead of keeping a guessed row.
      await queryClient.invalidateQueries({ queryKey: skillBackupKeys.all });
    },
  });
}
