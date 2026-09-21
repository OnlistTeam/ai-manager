import { invokeNative } from "../client";
import { operationIdSchema } from "../schemas/operation";
import {
  skillBackupListSchema,
  skillCatalogItemSchema,
  skillCatalogSchema,
  skillRepositoryDraftSchema,
  skillRepositoryListSchema,
  skillZipInstallOutcomeSchema,
  skillUpdateListSchema,
  type SkillCatalogItem,
  type SkillBackup,
  type SkillRepository,
  type SkillRepositoryDraft,
  type SkillZipInstallOutcome,
  type SkillUpdate,
} from "../schemas/skill";
import type { ToolId } from "../schemas/tool";

export const skills = {
  catalog(tool: ToolId): Promise<SkillCatalogItem[]> {
    return invokeNative("app_skill_catalog_list", skillCatalogSchema, { tool });
  },

  install(tool: ToolId, skill: SkillCatalogItem): Promise<string> {
    return invokeNative("app_skill_install", operationIdSchema, {
      tool,
      skill: skillCatalogItemSchema.parse(skill),
    });
  },

  installZip(tool: ToolId): Promise<SkillZipInstallOutcome> {
    return invokeNative("app_skill_zip_install", skillZipInstallOutcomeSchema, {
      tool,
    });
  },

  backups(): Promise<SkillBackup[]> {
    return invokeNative("app_skill_backups_list", skillBackupListSchema);
  },

  restoreBackup(tool: ToolId, backup: string): Promise<string> {
    return invokeNative("app_skill_backup_restore", operationIdSchema, {
      tool,
      backup,
    });
  },

  deleteBackup(backup: string): Promise<SkillBackup[]> {
    return invokeNative("app_skill_backup_delete", skillBackupListSchema, {
      backup,
    });
  },

  remove(tool: ToolId, skill: string): Promise<string> {
    return invokeNative("app_skill_remove", operationIdSchema, {
      tool,
      skill,
    });
  },

  checkUpdates(): Promise<SkillUpdate[]> {
    return invokeNative("app_skill_updates_check", skillUpdateListSchema);
  },

  update(tool: ToolId, skill: string): Promise<string> {
    return invokeNative("app_skill_update", operationIdSchema, { tool, skill });
  },

  repositories(): Promise<SkillRepository[]> {
    return invokeNative(
      "app_skill_repositories_list",
      skillRepositoryListSchema,
    );
  },

  saveRepository(repository: SkillRepositoryDraft): Promise<SkillRepository[]> {
    return invokeNative(
      "app_skill_repository_save",
      skillRepositoryListSchema,
      { repository: skillRepositoryDraftSchema.parse(repository) },
    );
  },

  removeRepository(repository: string): Promise<SkillRepository[]> {
    return invokeNative(
      "app_skill_repository_remove",
      skillRepositoryListSchema,
      { repository },
    );
  },
};
