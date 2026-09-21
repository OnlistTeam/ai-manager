import { useMutation, useQueryClient } from "@tanstack/react-query";
import {
  skillRepositoryKeys,
  type SkillRepositoryDraft,
} from "@/entities/skill-repository";
import { skillCatalogKeys } from "@/entities/skill-catalog";
import { native } from "@/native";

export function useSaveSkillRepository() {
  const queryClient = useQueryClient();
  return useMutation({
    mutationFn: (repository: SkillRepositoryDraft) =>
      native.skills.saveRepository(repository),
    onSuccess: (repositories) => {
      queryClient.setQueryData(skillRepositoryKeys.list(), repositories);
      void queryClient.invalidateQueries({ queryKey: skillCatalogKeys.all });
    },
  });
}

export function useRemoveSkillRepository() {
  const queryClient = useQueryClient();
  return useMutation({
    mutationFn: (repository: string) =>
      native.skills.removeRepository(repository),
    onSuccess: (repositories) => {
      queryClient.setQueryData(skillRepositoryKeys.list(), repositories);
      void queryClient.invalidateQueries({ queryKey: skillCatalogKeys.all });
    },
  });
}
