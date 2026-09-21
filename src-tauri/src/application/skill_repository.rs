//! Product use cases for configured Skill catalog sources.

use crate::compat::ccswitch::skill_catalog::SkillCatalogStore;
use crate::domain::{AppError, SkillRepository, SkillRepositoryDraft};

pub struct SkillRepositoryDirectory;

impl SkillRepositoryDirectory {
    pub fn list(app_handle: &tauri::AppHandle) -> Result<Vec<SkillRepository>, AppError> {
        SkillCatalogStore::open(app_handle)?.repositories()
    }

    pub fn save(
        app_handle: &tauri::AppHandle,
        draft: SkillRepositoryDraft,
    ) -> Result<Vec<SkillRepository>, AppError> {
        SkillCatalogStore::open(app_handle)?.save_repository(draft)
    }

    pub fn remove(
        app_handle: &tauri::AppHandle,
        id: &str,
    ) -> Result<Vec<SkillRepository>, AppError> {
        SkillCatalogStore::open(app_handle)?.remove_repository(id)
    }
}
