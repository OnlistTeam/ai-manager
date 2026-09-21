//! Skills data access object
//!
//! Provides CRUD operations for skills and skill repos.
//!
//! Unified management architecture since v3.10.0:
//! - Skills use a unified id primary key with per-app enable flags for four apps
//! - The actual files live in the AI Manager product AppData/skills/ and are synced to each app directory

use crate::app_config::{InstalledSkill, SkillApps};
use crate::database::{lock_conn, Database};
use crate::error::AppError;
use crate::services::skill::SkillRepo;
use indexmap::IndexMap;
use rusqlite::params;

impl Database {
    // ========== InstalledSkill CRUD ==========

    /// Get all installed skills
    pub fn get_all_installed_skills(&self) -> Result<IndexMap<String, InstalledSkill>, AppError> {
        let conn = lock_conn!(self.conn);
        let mut stmt = conn
            .prepare(
                "SELECT id, name, description, directory, repo_owner, repo_name, repo_branch,
                        readme_url, enabled_claude, enabled_codex, enabled_gemini, enabled_grokbuild,
                        enabled_opencode, enabled_hermes, installed_at, content_hash, updated_at
                 FROM skills ORDER BY name ASC",
            )
            .map_err(|e| AppError::Database(e.to_string()))?;

        let skill_iter = stmt
            .query_map([], |row| {
                Ok(InstalledSkill {
                    id: row.get(0)?,
                    name: row.get(1)?,
                    description: row.get(2)?,
                    directory: row.get(3)?,
                    repo_owner: row.get(4)?,
                    repo_name: row.get(5)?,
                    repo_branch: row.get(6)?,
                    readme_url: row.get(7)?,
                    apps: SkillApps {
                        claude: row.get(8)?,
                        codex: row.get(9)?,
                        gemini: row.get(10)?,
                        grokbuild: row.get(11)?,
                        opencode: row.get(12)?,
                        hermes: row.get(13)?,
                        pi: false,
                    },
                    installed_at: row.get(14)?,
                    content_hash: row.get(15)?,
                    updated_at: row.get::<_, i64>(16).unwrap_or(0),
                })
            })
            .map_err(|e| AppError::Database(e.to_string()))?;

        let mut skills = IndexMap::new();
        for skill_res in skill_iter {
            let skill = skill_res.map_err(|e| AppError::Database(e.to_string()))?;
            skills.insert(skill.id.clone(), skill);
        }
        Ok(skills)
    }

    /// Get a single installed skill
    pub fn get_installed_skill(&self, id: &str) -> Result<Option<InstalledSkill>, AppError> {
        let conn = lock_conn!(self.conn);
        let mut stmt = conn
            .prepare(
                "SELECT id, name, description, directory, repo_owner, repo_name, repo_branch,
                        readme_url, enabled_claude, enabled_codex, enabled_gemini, enabled_grokbuild,
                        enabled_opencode, enabled_hermes, installed_at, content_hash, updated_at
                 FROM skills WHERE id = ?1",
            )
            .map_err(|e| AppError::Database(e.to_string()))?;

        let result = stmt.query_row([id], |row| {
            Ok(InstalledSkill {
                id: row.get(0)?,
                name: row.get(1)?,
                description: row.get(2)?,
                directory: row.get(3)?,
                repo_owner: row.get(4)?,
                repo_name: row.get(5)?,
                repo_branch: row.get(6)?,
                readme_url: row.get(7)?,
                apps: SkillApps {
                    claude: row.get(8)?,
                    codex: row.get(9)?,
                    gemini: row.get(10)?,
                    grokbuild: row.get(11)?,
                    opencode: row.get(12)?,
                    hermes: row.get(13)?,
                    pi: false,
                },
                installed_at: row.get(14)?,
                content_hash: row.get(15)?,
                updated_at: row.get::<_, i64>(16).unwrap_or(0),
            })
        });

        match result {
            Ok(skill) => Ok(Some(skill)),
            Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
            Err(e) => Err(AppError::Database(e.to_string())),
        }
    }

    /// Save a skill (insert or update)
    pub fn save_skill(&self, skill: &InstalledSkill) -> Result<(), AppError> {
        let conn = lock_conn!(self.conn);
        conn.execute(
            "INSERT OR REPLACE INTO skills
             (id, name, description, directory, repo_owner, repo_name, repo_branch,
              readme_url, enabled_claude, enabled_codex, enabled_gemini, enabled_grokbuild, enabled_opencode, enabled_hermes,
              installed_at, content_hash, updated_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16, ?17)",
            params![
                skill.id,
                skill.name,
                skill.description,
                skill.directory,
                skill.repo_owner,
                skill.repo_name,
                skill.repo_branch,
                skill.readme_url,
                skill.apps.claude,
                skill.apps.codex,
                skill.apps.gemini,
                skill.apps.grokbuild,
                skill.apps.opencode,
                skill.apps.hermes,
                skill.installed_at,
                skill.content_hash,
                skill.updated_at,
            ],
        )
        .map_err(|e| AppError::Database(e.to_string()))?;
        Ok(())
    }

    /// Update only the metadata of an installed skill, leaving per-app enable states untouched.
    ///
    /// Unlike [`Self::save_skill`], this does not insert a missing record. An update can race
    /// with an enable-state toggle or an uninstall while the download is in flight, so the
    /// caller must preserve the `enabled_*` columns in the database and stop once the record
    /// has been deleted.
    pub fn update_skill_metadata(&self, skill: &InstalledSkill) -> Result<bool, AppError> {
        let conn = lock_conn!(self.conn);
        let affected = conn
            .execute(
                "UPDATE skills
                 SET name = ?1,
                     description = ?2,
                     directory = ?3,
                     repo_owner = ?4,
                     repo_name = ?5,
                     repo_branch = ?6,
                     readme_url = ?7,
                     installed_at = ?8,
                     content_hash = ?9,
                     updated_at = ?10
                 WHERE id = ?11 AND installed_at = ?12",
                params![
                    skill.name,
                    skill.description,
                    skill.directory,
                    skill.repo_owner,
                    skill.repo_name,
                    skill.repo_branch,
                    skill.readme_url,
                    skill.installed_at,
                    skill.content_hash,
                    skill.updated_at,
                    skill.id,
                    skill.installed_at,
                ],
            )
            .map_err(|e| AppError::Database(e.to_string()))?;
        Ok(affected > 0)
    }

    /// Delete a skill
    pub fn delete_skill(&self, id: &str) -> Result<bool, AppError> {
        let conn = lock_conn!(self.conn);
        let affected = conn
            .execute("DELETE FROM skills WHERE id = ?1", params![id])
            .map_err(|e| AppError::Database(e.to_string()))?;
        Ok(affected > 0)
    }

    /// Clear all skills (used by migrations)
    pub fn clear_skills(&self) -> Result<(), AppError> {
        let conn = lock_conn!(self.conn);
        conn.execute("DELETE FROM skills", [])
            .map_err(|e| AppError::Database(e.to_string()))?;
        Ok(())
    }

    /// Update the per-app enable state of a skill
    pub fn update_skill_apps(&self, id: &str, apps: &SkillApps) -> Result<bool, AppError> {
        let conn = lock_conn!(self.conn);
        let affected = conn
            .execute(
                "UPDATE skills SET enabled_claude = ?1, enabled_codex = ?2, enabled_gemini = ?3, enabled_grokbuild = ?4, enabled_opencode = ?5, enabled_hermes = ?6 WHERE id = ?7",
                params![apps.claude, apps.codex, apps.gemini, apps.grokbuild, apps.opencode, apps.hermes, id],
            )
            .map_err(|e| AppError::Database(e.to_string()))?;
        Ok(affected > 0)
    }

    /// Update the content hash and update timestamp of a skill
    pub fn update_skill_hash(
        &self,
        id: &str,
        content_hash: &str,
        updated_at: i64,
    ) -> Result<bool, AppError> {
        let conn = lock_conn!(self.conn);
        let affected = conn
            .execute(
                "UPDATE skills SET content_hash = ?1, updated_at = ?2 WHERE id = ?3",
                params![content_hash, updated_at, id],
            )
            .map_err(|e| AppError::Database(e.to_string()))?;
        Ok(affected > 0)
    }

    // ========== SkillRepo CRUD (unchanged) ==========

    /// Get all skill repos
    pub fn get_skill_repos(&self) -> Result<Vec<SkillRepo>, AppError> {
        let conn = lock_conn!(self.conn);
        let mut stmt = conn
            .prepare(
                "SELECT owner, name, branch, enabled FROM skill_repos ORDER BY owner ASC, name ASC",
            )
            .map_err(|e| AppError::Database(e.to_string()))?;

        let repo_iter = stmt
            .query_map([], |row| {
                Ok(SkillRepo {
                    owner: row.get(0)?,
                    name: row.get(1)?,
                    branch: row.get(2)?,
                    enabled: row.get(3)?,
                })
            })
            .map_err(|e| AppError::Database(e.to_string()))?;

        let mut repos = Vec::new();
        for repo_res in repo_iter {
            repos.push(repo_res.map_err(|e| AppError::Database(e.to_string()))?);
        }
        Ok(repos)
    }

    /// Save a skill repo
    pub fn save_skill_repo(&self, repo: &SkillRepo) -> Result<(), AppError> {
        let conn = lock_conn!(self.conn);
        conn.execute(
            "INSERT OR REPLACE INTO skill_repos (owner, name, branch, enabled) VALUES (?1, ?2, ?3, ?4)",
            params![repo.owner, repo.name, repo.branch, repo.enabled],
        )
        .map_err(|e| AppError::Database(e.to_string()))?;
        Ok(())
    }

    /// Delete a skill repo
    pub fn delete_skill_repo(&self, owner: &str, name: &str) -> Result<(), AppError> {
        let conn = lock_conn!(self.conn);
        conn.execute(
            "DELETE FROM skill_repos WHERE owner = ?1 AND name = ?2",
            params![owner, name],
        )
        .map_err(|e| AppError::Database(e.to_string()))?;
        Ok(())
    }

    /// Initialize the default skill repos (called at startup, once per database)
    pub fn init_default_skill_repos(&self) -> Result<usize, AppError> {
        const INITIALIZED_KEY: &str = "default_skill_repos_initialized";

        if self.get_bool_flag(INITIALIZED_KEY)? {
            return Ok(0);
        }

        // Respect choices that already existed before the upgrade and record the initialized
        // state so defaults are not restored after the user empties the list.
        if !self.get_skill_repos()?.is_empty() {
            self.set_setting(INITIALIZED_KEY, "true")?;
            return Ok(0);
        }

        let default_store = crate::services::skill::SkillStore::default();
        let mut count = 0;

        for repo in &default_store.repos {
            self.save_skill_repo(repo)?;
            count += 1;
            log::info!(
                "Initialized default skill repo: {}/{}",
                repo.owner,
                repo.name
            );
        }

        self.set_setting(INITIALIZED_KEY, "true")?;
        Ok(count)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app_config::AppType;

    fn skill(id: &str, name: &str, apps: SkillApps) -> InstalledSkill {
        InstalledSkill {
            id: id.to_string(),
            name: name.to_string(),
            description: Some(format!("{name} description")),
            directory: format!("{name}-directory"),
            repo_owner: Some("owner".to_string()),
            repo_name: Some("repo".to_string()),
            repo_branch: Some("main".to_string()),
            readme_url: Some(format!("https://example.com/{name}")),
            apps,
            installed_at: 1,
            content_hash: Some(format!("{name}-hash")),
            updated_at: 2,
        }
    }

    #[test]
    fn update_skill_metadata_preserves_enabled_apps() {
        let db = Database::memory().expect("memory db");
        let installed_apps = SkillApps::only(&AppType::Codex);
        let original = skill("owner/repo:skill", "original", installed_apps.clone());
        db.save_skill(&original).expect("seed skill");

        let mut candidate = skill(&original.id, "updated", SkillApps::only(&AppType::Claude));
        candidate.repo_branch = Some("next".to_string());
        candidate.updated_at = 42;

        assert!(db
            .update_skill_metadata(&candidate)
            .expect("update metadata"));

        let stored = db
            .get_installed_skill(&original.id)
            .expect("query skill")
            .expect("skill remains installed");
        assert_eq!(stored.name, candidate.name);
        assert_eq!(stored.description, candidate.description);
        assert_eq!(stored.directory, candidate.directory);
        assert_eq!(stored.repo_branch, candidate.repo_branch);
        assert_eq!(stored.readme_url, candidate.readme_url);
        assert_eq!(stored.content_hash, candidate.content_hash);
        assert_eq!(stored.updated_at, candidate.updated_at);
        assert_eq!(stored.apps, installed_apps);
    }

    #[test]
    fn update_skill_metadata_does_not_insert_missing_skill() {
        let db = Database::memory().expect("memory db");
        let candidate = skill(
            "owner/repo:missing",
            "missing",
            SkillApps::only(&AppType::Claude),
        );

        assert!(!db
            .update_skill_metadata(&candidate)
            .expect("missing update is not an error"));
        assert!(db
            .get_installed_skill(&candidate.id)
            .expect("query skill")
            .is_none());
    }

    #[test]
    fn update_skill_metadata_does_not_touch_reinstalled_generation() {
        let db = Database::memory().expect("memory db");
        let stale_update = skill(
            "owner/repo:skill",
            "stale-update",
            SkillApps::only(&AppType::Claude),
        );

        let mut reinstalled = skill(
            &stale_update.id,
            "reinstalled",
            SkillApps::only(&AppType::Gemini),
        );
        reinstalled.installed_at = stale_update.installed_at + 1;
        db.save_skill(&reinstalled).expect("seed reinstalled skill");

        assert!(!db
            .update_skill_metadata(&stale_update)
            .expect("stale generation update is not an error"));

        let stored = db
            .get_installed_skill(&reinstalled.id)
            .expect("query skill")
            .expect("reinstalled generation remains");
        assert_eq!(stored.name, reinstalled.name);
        assert_eq!(stored.installed_at, reinstalled.installed_at);
        assert_eq!(stored.apps, reinstalled.apps);
    }
}
