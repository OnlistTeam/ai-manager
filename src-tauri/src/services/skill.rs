//! Skills service layer
//!
//! Unified management architecture since v3.10.0:
//! - SSOT (single source of truth): `skills/` under the AI Manager product AppData
//! - Downloads go to the SSOT on install and are synced to each app directory on demand
//! - The database stores the install records and the enabled state

use anyhow::{anyhow, Context, Result};
use base64::{engine::general_purpose::STANDARD as BASE64_STANDARD, Engine as _};
use chrono::{DateTime, Utc};
use futures::{StreamExt, TryStreamExt};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::{HashMap, HashSet};
use std::fs;
use std::path::{Component, Path, PathBuf};
use std::sync::{Arc, OnceLock, RwLock, RwLockReadGuard, RwLockWriteGuard};
use std::time::{SystemTime, UNIX_EPOCH};
use tokio::time::timeout;

use crate::app_config::{AppType, InstalledSkill, SkillApps, UnmanagedSkill};
use crate::database::Database;
use crate::error::format_skill_error;

// ========== Skills state coordination ==========

/// Coordinates the database `skills` state with the filesystem SSOT.
///
/// Lock order is: global sync operation lock -> this lock -> database mutex.
/// Async install/update paths acquire the write guard only after downloads have
/// completed, so no non-Send std guard is held across an `.await`.
fn skill_state_lock() -> &'static RwLock<()> {
    static LOCK: OnceLock<RwLock<()>> = OnceLock::new();
    LOCK.get_or_init(|| RwLock::new(()))
}

pub(crate) fn skill_state_read_guard() -> RwLockReadGuard<'static, ()> {
    skill_state_lock().read().unwrap_or_else(|poisoned| {
        log::warn!("Skills state read lock was poisoned; recovering the protected state");
        poisoned.into_inner()
    })
}

pub(crate) fn skill_state_write_guard() -> RwLockWriteGuard<'static, ()> {
    skill_state_lock().write().unwrap_or_else(|poisoned| {
        log::warn!("Skills state write lock was poisoned; recovering the protected state");
        poisoned.into_inner()
    })
}

// ========== Data structures ==========

/// Skill sync method
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum SyncMethod {
    /// Automatic: prefer symlink, fall back to copy on failure
    #[default]
    Auto,
    /// Symbolic link (recommended, saves disk space)
    Symlink,
    /// File copy (compatibility mode)
    Copy,
}

/// Skill storage location (SSOT directory choice)
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum SkillStorageLocation {
    /// AI Manager managed directory (the historical serialized value is still `cc_switch`)
    #[default]
    CcSwitch,
    /// The unified Agent Skills standard directory (~/.agents/skills/)
    Unified,
}

/// A discoverable skill (from a repository)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DiscoverableSkill {
    /// Unique identifier: "owner/name:directory"
    pub key: String,
    /// Display name (parsed from SKILL.md)
    pub name: String,
    /// Skill description
    pub description: String,
    /// Directory name (last segment of the install path)
    pub directory: String,
    /// GitHub README URL
    #[serde(rename = "readmeUrl")]
    pub readme_url: Option<String>,
    /// Repository owner
    #[serde(rename = "repoOwner")]
    pub repo_owner: String,
    /// Repository name
    #[serde(rename = "repoName")]
    pub repo_name: String,
    /// Branch name
    #[serde(rename = "repoBranch")]
    pub repo_branch: String,
    /// Whether this repository snapshot was delivered over the trusted jsDelivr GitHub channel.
    #[serde(rename = "mirrorUsed", default)]
    pub mirror_used: bool,
}

/// Skill object (legacy API compatibility, internally uses DiscoverableSkill)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Skill {
    /// Unique identifier: "owner/name:directory" or "local:directory"
    pub key: String,
    /// Display name (parsed from SKILL.md)
    pub name: String,
    /// Skill description
    pub description: String,
    /// Directory name (last segment of the install path)
    pub directory: String,
    /// GitHub README URL
    #[serde(rename = "readmeUrl")]
    pub readme_url: Option<String>,
    /// Whether it is installed
    pub installed: bool,
    /// Repository owner
    #[serde(rename = "repoOwner")]
    pub repo_owner: Option<String>,
    /// Repository name
    #[serde(rename = "repoName")]
    pub repo_name: Option<String>,
    /// Branch name
    #[serde(rename = "repoBranch")]
    pub repo_branch: Option<String>,
    /// Whether this directory snapshot was delivered over the trusted jsDelivr GitHub channel.
    #[serde(rename = "mirrorUsed", default)]
    pub mirror_used: bool,
}

/// Repository config
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SkillRepo {
    /// GitHub user/organization name
    pub owner: String,
    /// Repository name
    pub name: String,
    /// Branch (defaults to "main")
    pub branch: String,
    /// Whether it is enabled
    pub enabled: bool,
}

/// Skill install state (legacy compatibility)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SkillState {
    /// Whether it is installed
    pub installed: bool,
    /// Install time
    #[serde(rename = "installedAt")]
    pub installed_at: DateTime<Utc>,
}

/// Persisted storage structure (repository config)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SkillStore {
    /// directory -> install state (legacy compatibility, unused in the new version)
    pub skills: HashMap<String, SkillState>,
    /// Repository list
    pub repos: Vec<SkillRepo>,
}

impl Default for SkillStore {
    fn default() -> Self {
        SkillStore {
            skills: HashMap::new(),
            repos: vec![
                SkillRepo {
                    owner: "anthropics".to_string(),
                    name: "skills".to_string(),
                    branch: "main".to_string(),
                    enabled: true,
                },
                SkillRepo {
                    owner: "ComposioHQ".to_string(),
                    name: "awesome-claude-skills".to_string(),
                    branch: "master".to_string(),
                    enabled: true,
                },
                SkillRepo {
                    owner: "cexll".to_string(),
                    name: "myclaude".to_string(),
                    branch: "master".to_string(),
                    enabled: true,
                },
                SkillRepo {
                    owner: "JimLiu".to_string(),
                    name: "baoyu-skills".to_string(),
                    branch: "main".to_string(),
                    enabled: true,
                },
            ],
        }
    }
}

/// Skill uninstall result
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SkillUninstallResult {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub backup_path: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub preserved_pi_path: Option<String>,
    #[serde(default, skip_serializing_if = "is_false")]
    pub pi_cleanup_incomplete: bool,
}

fn is_false(value: &bool) -> bool {
    !*value
}

/// Skill update check result
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SkillUpdateInfo {
    /// Skill ID
    pub id: String,
    /// Skill name
    pub name: String,
    /// Current local hash
    pub current_hash: Option<String>,
    /// Latest remote hash
    pub remote_hash: String,
}

/// Skill storage location migration result
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MigrationResult {
    pub migrated_count: usize,
    pub skipped_count: usize,
    pub errors: Vec<String>,
}

#[derive(Debug)]
enum PiSkillDeployment {
    Symlink { expected_target: PathBuf },
    Copy { expected_hash: String },
}

// ========== skills.sh API types ==========

/// Raw skills.sh API response
///
/// Note: the API naming is inconsistent (searchType is camelCase, duration_ms is snake_case), so
/// rename_all cannot be used and each field is specified individually.
#[derive(Debug, Clone, Deserialize)]
struct SkillsShApiResponse {
    pub query: String,
    pub skills: Vec<SkillsShApiSkill>,
    pub count: usize,
}

/// Raw skills.sh API skill entry
#[derive(Debug, Clone, Deserialize)]
struct SkillsShApiSkill {
    pub id: String,
    #[serde(rename = "skillId")]
    pub skill_id: String,
    pub name: String,
    pub installs: u64,
    pub source: String,
}

/// skills.sh search result (returned to the frontend)
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SkillsShSearchResult {
    pub skills: Vec<SkillsShDiscoverableSkill>,
    pub total_count: usize,
    pub query: String,
}

/// skills.sh installable skill (returned to the frontend)
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SkillsShDiscoverableSkill {
    pub key: String,
    pub name: String,
    pub directory: String,
    pub repo_owner: String,
    pub repo_name: String,
    pub repo_branch: String,
    pub installs: u64,
    pub readme_url: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SkillBackupEntry {
    pub backup_id: String,
    pub backup_path: String,
    pub created_at: i64,
    pub skill: InstalledSkill,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct SkillBackupMetadata {
    skill: InstalledSkill,
    backup_created_at: i64,
    source_path: String,
}

const SKILL_BACKUP_RETAIN_COUNT: usize = 20;

/// Repository archive extraction limits: entry count and total uncompressed bytes.
///
/// Archive bytes are fully controlled by a third party (repositories can be added via deeplink and the
/// branch can redirect the download to an attacker-uploaded release asset), so without a limit a
/// few-MB zip bomb could fill the disk. The values match the same protection in `webdav_sync/archive.rs`.
const MAX_ARCHIVE_ENTRIES: usize = 10_000;
const MAX_ARCHIVE_TOTAL_BYTES: u64 = 512 * 1024 * 1024;
/// A symlink target is just a path, a few dozen bytes; 4 KiB is a generous cap.
/// This cap is required: the `make_reader` of zip 2.4.2 does not truncate reads at the declared
/// uncompressed_size, so an entry flagged as a symlink whose deflate stream expands to several GB
/// would be read entirely into memory by `read_to_string`.
const MAX_SYMLINK_TARGET_BYTES: u64 = 4 * 1024;
/// Materializing a directory is billed as one directory block. Empty directories write no content
/// bytes but still consume inodes and disk blocks, so not billing them would allow unlimited directories.
const DIRECTORY_BUDGET_COST: u64 = 4096;
/// Compressed-size cap. The extraction budget only applies once the ZipArchive is built, and by then
/// the whole response body is already in memory, so the download step needs its own cap. Skill
/// repositories are Markdown, so a 128 MiB archive is already far beyond a normal size.
const MAX_ARCHIVE_DOWNLOAD_BYTES: u64 = 128 * 1024 * 1024;
/// A jsDelivr snapshot manifest is only paths/sizes/hashes; 16 MiB describes 10k files within the cap.
const MAX_SKILL_MIRROR_METADATA_BYTES: u64 = 16 * 1024 * 1024;
/// Mirror mode reads file by file but still shares the memory/download caps of the primary source.
const MAX_SKILL_MIRROR_TOTAL_BYTES: u64 = MAX_ARCHIVE_DOWNLOAD_BYTES;
const MAX_SKILL_MIRROR_CONCURRENCY: usize = 8;
const MAX_SKILL_MIRROR_PATH_BYTES: usize = 4096;
const MAX_SKILL_MIRROR_PATH_COMPONENTS: usize = 64;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum SkillRepoDelivery {
    Github,
    Jsdelivr,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum RepoTransferFailureClass {
    NotFound,
    RetryableTransport,
    Terminal,
}

#[derive(Debug)]
struct RepoTransferFailure {
    error: anyhow::Error,
    class: RepoTransferFailureClass,
}

impl RepoTransferFailure {
    fn retryable(error: impl Into<anyhow::Error>) -> Self {
        Self {
            error: error.into(),
            class: RepoTransferFailureClass::RetryableTransport,
        }
    }

    fn terminal(error: impl Into<anyhow::Error>) -> Self {
        Self {
            error: error.into(),
            class: RepoTransferFailureClass::Terminal,
        }
    }

    fn from_status(status: u16) -> Self {
        let status_text = status.to_string();
        let error = anyhow!(format_skill_error(
            "DOWNLOAD_FAILED",
            &[("status", &status_text)],
            match status {
                403 => Some("http403"),
                404 => Some("http404"),
                429 => Some("http429"),
                _ => Some("checkNetwork"),
            },
        ));
        Self {
            error,
            class: Self::classify_status(status),
        }
    }

    fn classify_status(status: u16) -> RepoTransferFailureClass {
        match status {
            404 => RepoTransferFailureClass::NotFound,
            408 | 429 | 500..=599 => RepoTransferFailureClass::RetryableTransport,
            _ => RepoTransferFailureClass::Terminal,
        }
    }
}

#[derive(Debug, Deserialize)]
struct JsdelivrFlatResponse {
    version: Option<String>,
    files: Vec<JsdelivrFlatFile>,
}

#[derive(Debug, Deserialize)]
struct JsdelivrFlatFile {
    name: String,
    hash: String,
    size: u64,
}

#[derive(Debug)]
struct ValidatedMirrorFile {
    path: PathBuf,
    url: url::Url,
    expected_branch: String,
    expected_hash: Vec<u8>,
    expected_size: u64,
}

/// Skill metadata (parsed from SKILL.md)
#[derive(Debug, Clone, Deserialize)]
pub struct SkillMetadata {
    pub name: Option<String>,
    pub description: Option<String>,
}

/// Enabled-app selection explicitly submitted by the frontend when importing an existing skill
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ImportSkillSelection {
    pub directory: String,
    #[serde(default)]
    pub apps: SkillApps,
}

#[derive(Debug, Clone, Deserialize)]
struct LegacySkillMigrationRow {
    directory: String,
    app_type: String,
}

// ========== ~/.agents/ lock file parsing ==========

/// Structure of the `~/.agents/.skill-lock.json` file
#[derive(Deserialize)]
struct AgentsLockFile {
    skills: HashMap<String, AgentsLockSkill>,
}

/// Information about a single skill in the lock file
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct AgentsLockSkill {
    source: Option<String>,
    source_type: Option<String>,
    source_url: Option<String>,
    skill_path: Option<String>,
    branch: Option<String>,
    source_branch: Option<String>,
}

#[derive(Debug, Clone)]
struct LockRepoInfo {
    owner: String,
    repo: String,
    skill_path: Option<String>,
    branch: Option<String>,
}

fn normalize_optional_branch(branch: Option<String>) -> Option<String> {
    branch.and_then(|b| {
        let trimmed = b.trim();
        if trimmed.is_empty() {
            None
        } else {
            Some(trimmed.to_string())
        }
    })
}

fn parse_branch_from_source_url(source_url: Option<&str>) -> Option<String> {
    let source_url = source_url?;
    let source_url = source_url.trim();
    if source_url.is_empty() {
        return None;
    }

    // Supports https://github.com/owner/repo/tree/<branch>/...
    if let Some((_, after_tree)) = source_url.split_once("/tree/") {
        let branch = after_tree
            .split('/')
            .next()
            .map(str::trim)
            .filter(|s| !s.is_empty())?;
        return Some(branch.to_string());
    }

    // Supports the URL fragment form: ...git#branch
    if let Some((_, fragment)) = source_url.split_once('#') {
        let branch = fragment
            .split('&')
            .next()
            .map(str::trim)
            .filter(|s| !s.is_empty())?;
        return Some(branch.to_string());
    }

    // Supports queries: ...?branch=xxx / ?ref=xxx
    if let Some((_, query)) = source_url.split_once('?') {
        for pair in query.split('&') {
            let Some((key, value)) = pair.split_once('=') else {
                continue;
            };
            if matches!(key, "branch" | "ref") {
                let branch = value.trim();
                if !branch.is_empty() {
                    return Some(branch.to_string());
                }
            }
        }
    }

    None
}

/// Get the `~/.agents/skills/` directory (returned when it exists)
fn get_agents_skills_dir() -> Option<PathBuf> {
    let dir = crate::config::get_home_dir().join(".agents").join("skills");
    dir.exists().then_some(dir)
}

/// Parse `~/.agents/.skill-lock.json` and return skill_name -> repository info
fn parse_agents_lock() -> HashMap<String, LockRepoInfo> {
    let path = crate::config::get_home_dir()
        .join(".agents")
        .join(".skill-lock.json");
    let content = match fs::read_to_string(&path) {
        Ok(c) => c,
        Err(e) => {
            if e.kind() == std::io::ErrorKind::NotFound {
                log::debug!("agents lock file not found: {}", path.display());
            } else {
                log::warn!(
                    "Failed to read the agents lock file ({}): {}",
                    path.display(),
                    e
                );
            }
            return HashMap::new();
        }
    };
    let lock: AgentsLockFile = match serde_json::from_str(&content) {
        Ok(l) => l,
        Err(e) => {
            log::warn!(
                "Failed to parse the agents lock file ({}): {}",
                path.display(),
                e
            );
            return HashMap::new();
        }
    };
    let parsed: HashMap<String, LockRepoInfo> = lock
        .skills
        .into_iter()
        .filter_map(|(name, skill)| {
            let source = skill.source?;
            if skill.source_type.as_deref() != Some("github") {
                return None;
            }
            let (owner, repo) = source.split_once('/')?;
            let branch = normalize_optional_branch(skill.branch)
                .or_else(|| normalize_optional_branch(skill.source_branch))
                .or_else(|| parse_branch_from_source_url(skill.source_url.as_deref()));
            Some((
                name,
                LockRepoInfo {
                    owner: owner.to_string(),
                    repo: repo.to_string(),
                    skill_path: skill.skill_path,
                    branch,
                },
            ))
        })
        .collect();
    log::info!(
        "agents lock file parsed, {} github skills recognized",
        parsed.len()
    );
    parsed
}

// ========== SkillService ==========

pub struct SkillService;

impl Default for SkillService {
    fn default() -> Self {
        Self::new()
    }
}

impl SkillService {
    pub fn new() -> Self {
        Self
    }

    /// Build the skill documentation URL (pointing at SKILL.md in the repository)
    ///
    /// Returns None for invalid coordinates: the value is stored in `readme_url` and the frontend's
    /// "view docs" opens it with `openExternal`, so a malicious branch could point it anywhere on github.com.
    fn build_skill_doc_url(
        owner: &str,
        repo: &str,
        branch: &str,
        doc_path: &str,
    ) -> Option<String> {
        if Self::validate_repo_ref(owner, repo, branch).is_err() {
            log::warn!("Skipping the doc link of an invalid repository coordinate: {owner}/{repo}@{branch}");
            return None;
        }
        Some(format!(
            "https://github.com/{owner}/{repo}/blob/{branch}/{doc_path}"
        ))
    }

    /// Extract the in-repository doc path from a legacy readme_url, supporting both `blob` and `tree` forms
    fn extract_doc_path_from_url(url: &str) -> Option<String> {
        let marker = if url.contains("/blob/") {
            "/blob/"
        } else if url.contains("/tree/") {
            "/tree/"
        } else {
            return None;
        };

        let (_, tail) = url.split_once(marker)?;
        let (_, path) = tail.split_once('/')?;
        if path.is_empty() {
            return None;
        }
        Some(path.to_string())
    }

    // ========== Path management ==========

    /// Get the SSOT directory (product AppData/skills or ~/.agents/skills/ depending on the setting)
    pub fn get_ssot_dir() -> Result<PathBuf> {
        let location = crate::settings::get_skill_storage_location();
        let dir = match location {
            SkillStorageLocation::CcSwitch => {
                crate::infrastructure::paths::product_data_dir().join("skills")
            }
            SkillStorageLocation::Unified => {
                crate::config::get_home_dir().join(".agents").join("skills")
            }
        };
        fs::create_dir_all(&dir)?;
        Ok(dir)
    }

    /// Get the product-private backup directory for skill uninstalls.
    fn get_backup_dir() -> Result<PathBuf> {
        let dir = crate::infrastructure::paths::product_data_dir().join("skill-backups");
        fs::create_dir_all(&dir)?;
        Ok(dir)
    }

    /// Get the skills directory of an app
    pub fn get_app_skills_dir(app: &AppType) -> Result<PathBuf> {
        // Directory override: prefer the override directory configured by the user in settings.json
        match app {
            AppType::Claude => {
                if let Some(custom) = crate::settings::get_claude_override_dir() {
                    return Ok(custom.join("skills"));
                }
            }
            AppType::ClaudeDesktop => {}
            AppType::Codex => {
                if let Some(custom) = crate::settings::get_codex_override_dir() {
                    return Ok(custom.join("skills"));
                }
            }
            AppType::Gemini => {
                if let Some(custom) = crate::settings::get_gemini_override_dir() {
                    return Ok(custom.join("skills"));
                }
            }
            AppType::GrokBuild => {
                if let Some(custom) = crate::settings::get_grok_override_dir() {
                    return Ok(custom.join("skills"));
                }
            }
            AppType::OpenCode => {
                if let Some(custom) = crate::settings::get_opencode_override_dir() {
                    return Ok(custom.join("skills"));
                }
            }
            AppType::OpenClaw => {
                if let Some(custom) = crate::settings::get_openclaw_override_dir() {
                    return Ok(custom.join("skills"));
                }
            }
            AppType::Hermes => {
                if let Some(custom) = crate::settings::get_hermes_override_dir() {
                    return Ok(custom.join("skills"));
                }
            }
            AppType::Pi => {
                return Ok(crate::pi_config::get_pi_agent_dir()?.join("skills"));
            }
        }

        // Default path: fall back to the standard location under the user's home directory.
        // get_home_dir() must be used (it can be overridden by AI_MANAGER_TEST_HOME): on Windows
        // dirs::home_dir() goes through the Known Folder API and tests cannot isolate the real user directory.
        let home = crate::config::get_home_dir();

        Ok(match app {
            AppType::Claude => home.join(".claude").join("skills"),
            AppType::ClaudeDesktop => home.join(".claude-desktop").join("skills"),
            AppType::Codex => home.join(".codex").join("skills"),
            AppType::Gemini => home.join(".gemini").join("skills"),
            AppType::GrokBuild => home.join(".grok").join("skills"),
            AppType::OpenCode => home.join(".config").join("opencode").join("skills"),
            AppType::OpenClaw => home.join(".openclaw").join("skills"),
            AppType::Hermes => crate::hermes_config::get_hermes_dir().join("skills"),
            AppType::Pi => crate::pi_config::get_pi_agent_dir()?.join("skills"),
        })
    }

    fn paths_alias(left: &Path, right: &Path) -> bool {
        if left == right {
            return true;
        }

        matches!(
            (left.canonicalize(), right.canonicalize()),
            (Ok(left), Ok(right)) if left == right
        )
    }

    fn paths_overlap(left: &Path, right: &Path) -> bool {
        let overlaps = |left: &Path, right: &Path| {
            left == right || left.starts_with(right) || right.starts_with(left)
        };
        if overlaps(left, right) {
            return true;
        }

        if let (Ok(left), Ok(right)) = (left.canonicalize(), right.canonicalize()) {
            if overlaps(&left, &right) {
                return true;
            }
        }

        // canonicalize() follows the final component and therefore fails for a
        // dangling symlink. Resolve the parents separately so two applications
        // cannot delete the same directory entry through aliased roots.
        let canonical_entry =
            |path: &Path| Some(path.parent()?.canonicalize().ok()?.join(path.file_name()?));
        matches!(
            (canonical_entry(left), canonical_entry(right)),
            (Some(left), Some(right)) if overlaps(&left, &right)
        )
    }

    fn ensure_distinct_skill_roots(ssot_dir: &Path, app_dir: &Path, app: &AppType) -> Result<()> {
        if Self::paths_alias(ssot_dir, app_dir) {
            return Err(anyhow!(
                "The skill storage directory must not be the same as the {app:?} skills directory: {}",
                ssot_dir.display()
            ));
        }
        Ok(())
    }

    fn get_distinct_app_skills_dir(ssot_dir: &Path, app: &AppType) -> Result<PathBuf> {
        let app_dir = Self::get_app_skills_dir(app)?;
        Self::ensure_distinct_skill_roots(ssot_dir, &app_dir, app)?;
        Ok(app_dir)
    }

    fn validate_skill_storage_destination(ssot_dir: &Path) -> Result<()> {
        for app in AppType::all() {
            if matches!(app, AppType::ClaudeDesktop) {
                continue;
            }
            let app_dir = Self::get_app_skills_dir(&app)?;
            Self::ensure_distinct_skill_roots(ssot_dir, &app_dir, &app)?;
        }
        Ok(())
    }

    // ========== Unified management methods ==========

    /// Get all installed skills
    pub fn get_all_installed(db: &Arc<Database>) -> Result<Vec<InstalledSkill>> {
        let mut skills = db.get_all_installed_skills()?;
        for skill in skills.values_mut() {
            skill.apps.pi = Self::skill_exists_in_app(&skill.directory, &AppType::Pi);
        }
        Ok(skills.into_values().collect())
    }

    /// Reuse an existing installation or reject a directory owned by another repo.
    /// The caller must hold [`skill_state_write_guard`] because this can update the
    /// database and materialized app directory.
    fn reuse_existing_install(
        db: &Arc<Database>,
        skill: &DiscoverableSkill,
        install_name: &str,
        current_app: &AppType,
    ) -> Result<Option<InstalledSkill>> {
        let existing_skills = db.get_all_installed_skills()?;
        for existing in existing_skills.values() {
            if !existing.directory.eq_ignore_ascii_case(install_name) {
                continue;
            }

            let same_repo = existing.repo_owner.as_deref() == Some(&skill.repo_owner)
                && existing.repo_name.as_deref() == Some(&skill.repo_name);
            if same_repo {
                let mut updated = existing.clone();
                updated.apps.set_enabled_for(current_app, true);
                db.save_skill(&updated)?;
                Self::sync_to_app_dir(&updated.directory, current_app)?;
                log::info!(
                    "Skill {} already exists, updating the {:?} enabled state",
                    updated.name,
                    current_app
                );
                return Ok(Some(updated));
            }

            return Err(anyhow!(format_skill_error(
                "SKILL_DIRECTORY_CONFLICT",
                &[
                    ("directory", install_name),
                    (
                        "existing_repo",
                        &format!(
                            "{}/{}",
                            existing.repo_owner.as_deref().unwrap_or("unknown"),
                            existing.repo_name.as_deref().unwrap_or("unknown")
                        )
                    ),
                    (
                        "new_repo",
                        &format!("{}/{}", skill.repo_owner, skill.repo_name)
                    ),
                ],
                Some("uninstallFirst"),
            )));
        }

        Ok(None)
    }

    /// Install a skill
    ///
    /// Flow:
    /// 1. Download into the SSOT directory
    /// 2. Save to the database
    /// 3. Sync to the enabled app directories
    pub async fn install(
        &self,
        db: &Arc<Database>,
        skill: &DiscoverableSkill,
        current_app: &AppType,
    ) -> Result<InstalledSkill> {
        let ssot_dir = Self::get_ssot_dir()?;

        // Multi-level directories (such as a/b/c) are allowed, but must be a safe relative path.
        let source_rel = Self::sanitize_skill_source_path(&skill.directory).ok_or_else(|| {
            anyhow!(format_skill_error(
                "INVALID_SKILL_DIRECTORY",
                &[("directory", &skill.directory)],
                Some("checkZipContent"),
            ))
        })?;
        // The install directory name always uses the last segment, avoiding nested directories in the SSOT.
        let install_name = source_rel
            .file_name()
            .and_then(|name| Self::sanitize_install_name(&name.to_string_lossy()))
            .ok_or_else(|| {
                anyhow!(format_skill_error(
                    "INVALID_SKILL_DIRECTORY",
                    &[("directory", &skill.directory)],
                    Some("checkZipContent"),
                ))
            })?;

        // Fast path for an existing installation. The write guard makes the DB
        // row and app projection indivisible from a concurrent cloud snapshot.
        {
            let _state_guard = skill_state_write_guard();
            if let Some(existing) =
                Self::reuse_existing_install(db, skill, &install_name, current_app)?
            {
                return Ok(existing);
            }
        }

        let dest = ssot_dir.join(&install_name);

        let mut repo_branch = skill.repo_branch.clone();
        // Doc path derived from the actually resolved source directory (only available on a real download+parse)
        let mut resolved_doc_path: Option<String> = None;
        let mut downloaded_source: Option<(tempfile::TempDir, PathBuf)> = None;

        // Skip the download when it already exists
        if !dest.exists() {
            let repo = SkillRepo {
                owner: skill.repo_owner.clone(),
                name: skill.repo_name.clone(),
                branch: skill.repo_branch.clone(),
                enabled: true,
            };

            // Download the repository
            let (temp_guard, used_branch, delivery) = timeout(
                std::time::Duration::from_secs(60),
                self.download_repo(&repo),
            )
            .await
            .map_err(|_| {
                anyhow!(format_skill_error(
                    "DOWNLOAD_TIMEOUT",
                    &[
                        ("owner", &repo.owner),
                        ("name", &repo.name),
                        ("timeout", "60")
                    ],
                    Some("checkNetwork"),
                ))
            })??;
            if delivery == SkillRepoDelivery::Jsdelivr {
                log::info!(
                    "Skill {}/{} was downloaded over the trusted jsDelivr GitHub channel this time",
                    repo.owner,
                    repo.name
                );
            }
            let temp_dir = temp_guard.path();
            repo_branch = used_branch;

            // Copy into the SSOT
            let source =
                Self::resolve_skill_source_dir(temp_dir, &skill.directory).ok_or_else(|| {
                    let missing = temp_dir.join(&source_rel).display().to_string();
                    anyhow!(format_skill_error(
                        "SKILL_DIR_NOT_FOUND",
                        &[("path", &missing)],
                        Some("checkRepoUrl"),
                    ))
                })?;

            let canonical_temp = temp_dir
                .canonicalize()
                .unwrap_or_else(|_| temp_dir.to_path_buf());
            let canonical_source = source.canonicalize().map_err(|_| {
                anyhow!(format_skill_error(
                    "SKILL_DIR_NOT_FOUND",
                    &[("path", &source.display().to_string())],
                    Some("checkRepoUrl"),
                ))
            })?;
            if !canonical_source.starts_with(&canonical_temp) || !canonical_source.is_dir() {
                return Err(anyhow!(format_skill_error(
                    "INVALID_SKILL_DIRECTORY",
                    &[("directory", &skill.directory)],
                    Some("checkZipContent"),
                )));
            }

            // Derive the doc path from the actually resolved source directory - the skills.sh directory
            // is only the skillId (last segment), so naive joining loses the path and 404s (#6111)
            resolved_doc_path = Self::doc_path_for_source(&canonical_temp, &canonical_source);

            downloaded_source = Some((temp_guard, canonical_source));

            // Use the branch that actually downloaded, so readme_url / repo_branch match the real branch.
            if repo_branch != skill.repo_branch {
                log::info!(
                    "Skill {}/{} branch auto fallback: {} -> {}",
                    skill.repo_owner,
                    skill.repo_name,
                    skill.repo_branch,
                    repo_branch
                );
            }
        }

        let doc_path = Self::choose_doc_path(
            resolved_doc_path,
            skill.readme_url.as_deref(),
            &skill.directory,
        );

        let readme_url =
            Self::build_skill_doc_url(&skill.repo_owner, &skill.repo_name, &repo_branch, &doc_path);

        // Re-check after the network download: another install/uninstall may have
        // completed while the lock was intentionally released around `.await`.
        let _state_guard = skill_state_write_guard();
        if let Some(existing) = Self::reuse_existing_install(db, skill, &install_name, current_app)?
        {
            return Ok(existing);
        }

        if !dest.exists() {
            let source = downloaded_source
                .as_ref()
                .map(|(_, source)| source)
                .ok_or_else(|| anyhow!("Skill directory changed during install; please retry"))?;
            Self::preflight_install_destination(source, &install_name, current_app)?;
            Self::copy_dir_recursive(source, &dest)?;
        }

        // Create the InstalledSkill record
        // Compute the content hash
        let content_hash = Self::compute_dir_hash(&dest).map(Some).unwrap_or_else(|e| {
            log::warn!("Failed to compute content hash for {}: {e}", install_name);
            None
        });

        let installed_skill = InstalledSkill {
            id: skill.key.clone(),
            name: skill.name.clone(),
            description: if skill.description.is_empty() {
                None
            } else {
                Some(skill.description.clone())
            },
            directory: install_name.clone(),
            repo_owner: Some(skill.repo_owner.clone()),
            repo_name: Some(skill.repo_name.clone()),
            repo_branch: Some(repo_branch),
            readme_url,
            apps: SkillApps::only(current_app),
            installed_at: chrono::Utc::now().timestamp(),
            content_hash,
            updated_at: 0,
        };

        Self::persist_and_sync_new_skill(db, &installed_skill, current_app)?;

        log::info!(
            "Skill {} installed successfully, enabled for {:?}",
            installed_skill.name,
            current_app
        );

        Ok(installed_skill)
    }

    /// Uninstall a skill
    ///
    /// Flow:
    /// 1. Delete from every app directory
    /// 2. Delete from the SSOT
    /// 3. Delete from the database
    pub fn uninstall(db: &Arc<Database>, id: &str) -> Result<SkillUninstallResult> {
        let _state_guard = skill_state_write_guard();

        // Read the skill info
        let skill = db
            .get_installed_skill(id)?
            .ok_or_else(|| anyhow!("Skill not found: {id}"))?;

        // The DB row may be polluted by a sync import (a remote snapshot is written with raw SQL,
        // bypassing the install-time validation), or be leftover dirt from before sanitize_install_name
        // was introduced in v3.11.0 (the old scan did not filter dot directories, so `.github/SKILL.md` was stored as `.github`).
        //
        // When the guard fails, **skip every filesystem operation but still delete the DB row**:
        // `db.delete_skill` is called only here in the whole project and is not exposed as a command, so
        // returning Err here would leave the user unable to remove the record from the UI and force
        // manual SQLite edits. The goal is "do not touch dangerous paths", not "lock the user into a bad state".
        let (backup_path, preserved_pi_path, pi_cleanup_incomplete) =
            match Self::require_valid_directory(&skill.directory) {
                Ok(directory) => {
                    let ssot_dir = Self::get_ssot_dir()?;
                    let source = ssot_dir.join(&directory);
                    let mut preserved_pi_path: Option<PathBuf> = None;
                    let mut pi_cleanup_incomplete = false;
                    let mut pi_removal_path = None;

                    match Self::get_app_skills_dir(&AppType::Pi) {
                        Ok(pi_dir) => {
                            let destination = pi_dir.join(&directory);
                            if Self::paths_alias(&ssot_dir, &pi_dir) {
                                // Pi uses the SSOT directly, so there is no second copy; deleting the SSOT completes the cleanup.
                                log::debug!(
                                    "The Pi skills directory of skill {id} is the same as the SSOT"
                                );
                            } else {
                                // Keep the target path even when it does not exist right now; another
                                // process may sync this skill during the backup, so recheck before deleting the SSOT.
                                pi_removal_path = Some(destination.clone());
                                if destination.exists() || Self::is_symlink(&destination) {
                                    if let Err(err) = Self::inspect_pi_skill_destination(
                                        &source,
                                        &destination,
                                        &directory,
                                    ) {
                                        log::warn!(
                                            "Skipping Pi cleanup while uninstalling skill {id}, keeping {}: {err}",
                                            destination.display()
                                        );
                                        preserved_pi_path = Some(destination);
                                        pi_removal_path = None;
                                        pi_cleanup_incomplete = true;
                                    }
                                }
                            }
                        }
                        Err(err) => {
                            log::warn!(
                                "Cannot resolve the Pi skills directory while uninstalling skill {id}, skipping Pi cleanup: {err}"
                            );
                            pi_cleanup_incomplete = true;
                        }
                    }

                    let backup_path = Self::create_uninstall_backup_excluding(
                        &skill,
                        preserved_pi_path.as_deref(),
                    )?
                    .map(|path| path.to_string_lossy().to_string());

                    // The Pi directory may hold a skill of the same name maintained by the user. Before
                    // deleting the SSOT, only remove copies verifiable as CC Switch deployments; keep the rest and return a warning.
                    if let Some(destination) = pi_removal_path {
                        let removal =
                            Self::remove_verified_pi_destination(&source, &destination, &directory);
                        if let Err(err) = removal {
                            log::warn!("Pi cleanup failed while uninstalling skill {id}, continuing the uninstall: {err}");
                            pi_cleanup_incomplete = true;
                            if destination.exists() || Self::is_symlink(&destination) {
                                preserved_pi_path = Some(destination);
                            }
                        }
                    }

                    // Other apps keep the existing per-item fault tolerance.
                    for app in AppType::all() {
                        if matches!(app, AppType::Pi) {
                            continue;
                        }
                        let _ = Self::remove_from_app_preserving(
                            &directory,
                            &app,
                            preserved_pi_path.as_deref(),
                        );
                    }

                    // Delete from the SSOT
                    let skill_path = ssot_dir.join(&directory);
                    let overlaps_preserved_pi = preserved_pi_path
                        .as_deref()
                        .is_some_and(|path| Self::paths_overlap(&skill_path, path));
                    if overlaps_preserved_pi {
                        log::warn!("The SSOT path of skill {id} overlaps a preserved Pi copy, skipping the file deletion");
                    } else if skill_path.exists() {
                        fs::remove_dir_all(&skill_path)?;
                    }
                    (backup_path, preserved_pi_path, pi_cleanup_incomplete)
                }
                Err(err) => {
                    log::warn!(
                    "The directory of skill {id} is invalid ({:?}), skipping the file cleanup and only deleting the database record: {err}",
                    skill.directory
                );
                    (None, None, true)
                }
            };

        // Delete from the database
        db.delete_skill(id)?;

        log::info!(
            "Skill {} uninstalled successfully{}",
            skill.name,
            backup_path
                .as_deref()
                .map(|path| format!(", backup: {path}"))
                .unwrap_or_default()
        );

        Ok(SkillUninstallResult {
            backup_path,
            preserved_pi_path: preserved_pi_path.map(|path| path.to_string_lossy().to_string()),
            pi_cleanup_incomplete,
        })
    }

    // ========== Update detection ==========

    /// Compute the SHA-256 hash of the directory content
    ///
    /// Recursively walks every non-hidden file in the directory, sorts them by relative path, and
    /// feeds "relative path\0content\0" per file into the same hasher.
    pub fn compute_dir_hash(dir: &Path) -> Result<String> {
        use sha2::{Digest, Sha256};

        let mut files: Vec<PathBuf> = Vec::new();
        Self::collect_files_for_hash(dir, dir, &mut files)?;
        files.sort();

        let mut hasher = Sha256::new();
        for file_path in &files {
            let relative = file_path.strip_prefix(dir).unwrap_or(file_path);
            let rel_str = relative.to_string_lossy().replace('\\', "/");
            hasher.update(rel_str.as_bytes());
            hasher.update(b"\0");
            let content = fs::read(file_path)
                .with_context(|| format!("Failed to read the file: {}", file_path.display()))?;
            hasher.update(&content);
            hasher.update(b"\0");
        }

        Ok(format!("{:x}", hasher.finalize()))
    }

    /// Recursively collect every non-hidden file in a directory
    #[allow(clippy::only_used_in_recursion)]
    fn collect_files_for_hash(base: &Path, current: &Path, files: &mut Vec<PathBuf>) -> Result<()> {
        let entries = fs::read_dir(current)
            .with_context(|| format!("Failed to read the directory: {}", current.display()))?;
        for entry in entries {
            let entry = entry?;
            let name = entry.file_name().to_string_lossy().to_string();
            if name.starts_with('.') {
                continue;
            }
            let path = entry.path();
            if path.is_dir() {
                Self::collect_files_for_hash(base, &path, files)?;
            } else {
                files.push(path);
            }
        }
        Ok(())
    }

    /// The destructive operations of a Pi copy deployment need a full directory tree comparison and
    /// cannot reuse the update-detection hash: hidden files, empty directories or a file type change all mean the user edited the native directory.
    fn compute_pi_deployment_hash(dir: &Path) -> Result<String> {
        use sha2::{Digest, Sha256};

        let mut entries = Vec::new();
        Self::collect_tree_entries(dir, &mut entries)?;
        entries.sort();

        let mut hasher = Sha256::new();
        for path in entries {
            let relative = path.strip_prefix(dir).unwrap_or(&path);
            hasher.update(relative.to_string_lossy().replace('\\', "/").as_bytes());
            hasher.update(b"\0");

            let metadata = fs::symlink_metadata(&path)?;
            let file_type = metadata.file_type();
            if file_type.is_symlink() {
                hasher.update(b"link\0");
                hasher.update(fs::read_link(&path)?.to_string_lossy().as_bytes());
            } else if file_type.is_dir() {
                hasher.update(b"dir\0");
            } else if file_type.is_file() {
                hasher.update(b"file\0");
                hasher.update(fs::read(&path)?);
            } else {
                hasher.update(b"other\0");
            }
            hasher.update(b"\0");
        }

        Ok(format!("{:x}", hasher.finalize()))
    }

    fn collect_tree_entries(current: &Path, entries: &mut Vec<PathBuf>) -> Result<()> {
        for entry in fs::read_dir(current)
            .with_context(|| format!("Failed to read the directory: {}", current.display()))?
        {
            let entry = entry?;
            let path = entry.path();
            let file_type = entry.file_type()?;
            entries.push(path.clone());
            if file_type.is_dir() {
                Self::collect_tree_entries(&path, entries)?;
            }
        }
        Ok(())
    }

    /// Decide which local hash check_updates should use.
    ///
    /// The order matters: confirm the SSOT directory exists before trusting the content_hash cached
    /// in the database. After restoring a database backup on a new machine the skill files do not
    /// travel with it, so the cached hash is present while the directory is gone; reading the cache
    /// first would report such skills as "up to date" and hide the missing state forever. A missing
    /// directory returns None, never equals the remote hash, and update_skill rebuilds it on click.
    ///
    /// Returns `(hash, freshly_computed)`; freshly_computed means the hash was computed on the spot and
    /// the caller should write it back into the database cache.
    pub(crate) fn local_hash_for_update_check(
        ssot_dir: &Path,
        raw_directory: &str,
        cached_hash: Option<&str>,
    ) -> Option<(String, bool)> {
        // A dirty directory would make compute_dir_hash walk an arbitrary directory, and the hash result
        // leaks a little information through the "has updates" UI state; when the path cannot be built
        // safely we must not report "updatable" either - update_skill hard-errors on the same check, so the cache is reused.
        let directory = match Self::require_valid_directory(raw_directory) {
            Ok(d) => d,
            Err(err) => {
                log::warn!("Skill directory is invalid, skipping the local directory check: {err}");
                return cached_hash.map(|h| (h.to_string(), false));
            }
        };

        let local_dir = ssot_dir.join(&directory);
        if !local_dir.exists() {
            return None;
        }

        if let Some(h) = cached_hash {
            return Some((h.to_string(), false));
        }

        match Self::compute_dir_hash(&local_dir) {
            Ok(h) => Some((h, true)),
            Err(_) => None,
        }
    }

    /// Check for updates of every installed skill
    ///
    /// Only skills with a repo_owner are checked (local skills are skipped), grouped by repository to
    /// avoid downloading the same repository twice.
    pub async fn check_updates(&self, db: &Arc<Database>) -> Result<Vec<SkillUpdateInfo>> {
        let skills = db.get_all_installed_skills()?;
        let mut updates = Vec::new();

        // Group by (owner, name, branch)
        let mut repo_groups: HashMap<(String, String, String), Vec<InstalledSkill>> =
            HashMap::new();

        for skill in skills.into_values() {
            let (owner, name, branch) =
                match (&skill.repo_owner, &skill.repo_name, &skill.repo_branch) {
                    (Some(o), Some(n), Some(b)) => (o.clone(), n.clone(), b.clone()),
                    (Some(o), Some(n), None) => (o.clone(), n.clone(), "main".to_string()),
                    _ => continue,
                };
            repo_groups
                .entry((owner, name, branch))
                .or_default()
                .push(skill);
        }

        let ssot_dir = Self::get_ssot_dir()?;

        for ((owner, name, branch), group_skills) in &repo_groups {
            let repo = SkillRepo {
                owner: owner.clone(),
                name: name.clone(),
                branch: branch.clone(),
                enabled: true,
            };

            // Download the repository ZIP
            let (temp_guard, _used_branch, delivery) = match timeout(
                std::time::Duration::from_secs(60),
                self.download_repo(&repo),
            )
            .await
            {
                Ok(Ok(result)) => result,
                Ok(Err(e)) => {
                    log::warn!(
                        "Failed to download {}/{} while checking for updates: {e}",
                        owner,
                        name
                    );
                    continue;
                }
                Err(_) => {
                    log::warn!(
                        "Timed out downloading {}/{} while checking for updates",
                        owner,
                        name
                    );
                    continue;
                }
            };
            if delivery == SkillRepoDelivery::Jsdelivr {
                log::info!(
                    "Skill update check {}/{} was downloaded over the trusted jsDelivr GitHub channel this time",
                    owner,
                    name
                );
            }
            let temp_dir = temp_guard.path();

            // Scan every skill directory in the repository
            let mut remote_skills: Vec<DiscoverableSkill> = Vec::new();
            let _ = self.scan_dir_recursive(temp_dir, temp_dir, &repo, &mut remote_skills);

            // Remote I/O is complete. Stabilize the local DB + SSOT while hashes
            // are read and any missing hash metadata is backfilled.
            let _state_guard = skill_state_read_guard();

            for skill in group_skills {
                // Find the matching skill directory in the remote repository
                let remote_match = remote_skills.iter().find(|rs| {
                    // Match rule: the last segment of the install name
                    let remote_install_name =
                        rs.directory.rsplit('/').next().unwrap_or(&rs.directory);
                    remote_install_name.eq_ignore_ascii_case(&skill.directory)
                });

                let remote_skill_dir = match remote_match {
                    Some(rs) => match Self::resolve_skill_source_dir(temp_dir, &rs.directory) {
                        Some(path) => path,
                        None => continue,
                    },
                    None => continue,
                };

                let remote_hash = match Self::compute_dir_hash(&remote_skill_dir) {
                    Ok(h) => h,
                    Err(e) => {
                        log::warn!("Failed to compute the remote hash {}: {e}", skill.id);
                        continue;
                    }
                };

                let local_hash = match Self::local_hash_for_update_check(
                    &ssot_dir,
                    &skill.directory,
                    skill.content_hash.as_deref(),
                ) {
                    Some((h, freshly_computed)) => {
                        if freshly_computed {
                            let _ = db.update_skill_hash(&skill.id, &h, 0);
                        }
                        Some(h)
                    }
                    None => None,
                };

                if local_hash.as_deref() != Some(&remote_hash) {
                    updates.push(SkillUpdateInfo {
                        id: skill.id.clone(),
                        name: skill.name.clone(),
                        current_hash: local_hash,
                        remote_hash,
                    });
                }
            }
        }

        Ok(updates)
    }

    /// Persist the updated skill metadata and re-read the authoritative per-app enabled state from the database.
    ///
    /// The update involves a network download during which the user may toggle the enabled state or
    /// uninstall the skill. A DAO that only updates existing records must be used, so an old snapshot
    /// cannot overwrite `enabled_*` and an uninstalled record is not reinserted.
    fn persist_updated_skill_metadata(
        db: &Arc<Database>,
        updated_skill: &InstalledSkill,
    ) -> Result<InstalledSkill> {
        if !db.update_skill_metadata(updated_skill)? {
            return Err(anyhow!("Skill no longer installed: {}", updated_skill.id));
        }

        db.get_installed_skill(&updated_skill.id)?
            .ok_or_else(|| anyhow!("Skill no longer installed: {}", updated_skill.id))
    }

    /// Update a single skill (re-download and replace the local files)
    pub async fn update_skill(&self, db: &Arc<Database>, skill_id: &str) -> Result<InstalledSkill> {
        let mut skill = db
            .get_installed_skill(skill_id)?
            .ok_or_else(|| anyhow!("Skill not found: {skill_id}"))?;
        skill.apps.pi = Self::skill_exists_in_app(&skill.directory, &AppType::Pi);

        // The three dangerous operations below all build paths from directory: the backup source (copies
        // an arbitrary directory into the backup area and lists it in the UI), remove_dir_all (deletes an
        // arbitrary directory) and copy_dir_recursive (writes remote repository content to an arbitrary path). Validation must come first.
        Self::require_valid_directory(&skill.directory)?;

        let (owner, name, branch) = match (&skill.repo_owner, &skill.repo_name) {
            (Some(o), Some(n)) => (
                o.clone(),
                n.clone(),
                skill
                    .repo_branch
                    .clone()
                    .unwrap_or_else(|| "main".to_string()),
            ),
            _ => return Err(anyhow!("Cannot update local skill: {skill_id}")),
        };

        let repo = SkillRepo {
            owner: owner.clone(),
            name: name.clone(),
            branch: branch.clone(),
            enabled: true,
        };

        let ssot_dir = Self::get_ssot_dir()?;
        if skill.apps.pi {
            Self::get_distinct_app_skills_dir(&ssot_dir, &AppType::Pi)?;
        }

        // Download the repository
        let (temp_guard, used_branch, delivery) = timeout(
            std::time::Duration::from_secs(60),
            self.download_repo(&repo),
        )
        .await
        .map_err(|_| {
            anyhow!(format_skill_error(
                "DOWNLOAD_TIMEOUT",
                &[("owner", &owner), ("name", &name), ("timeout", "60")],
                Some("checkNetwork"),
            ))
        })??;
        if delivery == SkillRepoDelivery::Jsdelivr {
            log::info!(
                "Skill update {}/{} was downloaded over the trusted jsDelivr GitHub channel this time",
                owner,
                name
            );
        }
        let temp_dir = temp_guard.path();

        // Find the skill source directory in the extracted repository
        let mut remote_skills: Vec<DiscoverableSkill> = Vec::new();
        let _ = self.scan_dir_recursive(temp_dir, temp_dir, &repo, &mut remote_skills);

        let remote_match = remote_skills
            .iter()
            .find(|rs| {
                let remote_install_name = rs.directory.rsplit('/').next().unwrap_or(&rs.directory);
                remote_install_name.eq_ignore_ascii_case(&skill.directory)
            })
            .ok_or_else(|| {
                anyhow!(format_skill_error(
                    "SKILL_DIR_NOT_FOUND",
                    &[("path", &skill.directory)],
                    Some("checkRepoUrl"),
                ))
            })?;

        let source =
            Self::resolve_skill_source_dir(temp_dir, &remote_match.directory).ok_or_else(|| {
                let missing = temp_dir.join(&remote_match.directory).display().to_string();
                anyhow!(format_skill_error(
                    "SKILL_DIR_NOT_FOUND",
                    &[("path", &missing)],
                    Some("checkRepoUrl"),
                ))
            })?;

        // Downloads do not mutate local state, so acquire only now and hold the
        // guard through the SSOT replacement, DB metadata update, and app sync.
        let _state_guard = skill_state_write_guard();

        // The user may have uninstalled this skill during the download and scan. The record must be
        // re-confirmed before any backup, delete or copy; otherwise, even though the final metadata
        // UPDATE would notice the missing row, this code would already have recreated the uninstalled SSOT directory.
        let mut current_skill = db
            .get_installed_skill(&skill.id)?
            .ok_or_else(|| anyhow!("Skill no longer installed: {}", skill.id))?;
        if current_skill.directory != skill.directory
            || current_skill.repo_owner != skill.repo_owner
            || current_skill.repo_name != skill.repo_name
            || current_skill.repo_branch != skill.repo_branch
            || current_skill.installed_at != skill.installed_at
        {
            return Err(anyhow!("Skill changed during update: {}", skill.id));
        }
        Self::require_valid_directory(&current_skill.directory)?;
        current_skill.apps.pi = Self::skill_exists_in_app(&current_skill.directory, &AppType::Pi);
        let skill = current_skill;

        let dest = ssot_dir.join(&skill.directory);
        let pi_deployment = if skill.apps.pi {
            let pi_dir = Self::get_distinct_app_skills_dir(&ssot_dir, &AppType::Pi)?;
            let pi_destination = pi_dir.join(&skill.directory);
            Self::inspect_pi_skill_destination(&dest, &pi_destination, &skill.directory)?
        } else {
            None
        };

        // Back up the old files
        let _ = Self::create_uninstall_backup(&skill);

        // Delete the old SSOT directory and copy the new files
        if dest.exists() {
            fs::remove_dir_all(&dest)?;
        }
        Self::copy_dir_recursive(&source, &dest)?;

        // Compute the new hash + parse the new metadata
        let new_hash = Self::compute_dir_hash(&dest).ok();
        let skill_md = dest.join("SKILL.md");
        let (new_name, new_description) = Self::read_skill_name_desc(&skill_md, &skill.directory);

        // Update readme_url
        let doc_path = skill
            .readme_url
            .as_deref()
            .and_then(Self::extract_doc_path_from_url)
            .unwrap_or_else(|| format!("{}/SKILL.md", skill.directory.trim_end_matches('/')));
        let readme_url = Self::build_skill_doc_url(&owner, &name, &used_branch, &doc_path);

        let updated_metadata = InstalledSkill {
            id: skill.id.clone(),
            name: new_name,
            description: new_description,
            directory: skill.directory.clone(),
            repo_owner: skill.repo_owner.clone(),
            repo_name: skill.repo_name.clone(),
            repo_branch: Some(used_branch),
            readme_url,
            apps: skill.apps.clone(),
            installed_at: skill.installed_at,
            content_hash: new_hash,
            updated_at: chrono::Utc::now().timestamp(),
        };

        if let Some(deployment) = pi_deployment {
            let pi_destination =
                Self::get_app_skills_dir(&AppType::Pi)?.join(&updated_metadata.directory);
            Self::refresh_pi_skill_destination(
                &dest,
                &pi_destination,
                &updated_metadata.directory,
                &deployment,
            )?;
        }

        let mut updated_skill = Self::persist_updated_skill_metadata(db, &updated_metadata)?;
        updated_skill.apps.pi = Self::skill_exists_in_app(&updated_skill.directory, &AppType::Pi);

        // Sync to every enabled app directory
        for app in updated_skill.apps.enabled_apps() {
            if matches!(app, AppType::Pi) {
                continue;
            }
            if let Err(e) = Self::sync_to_app_dir(&updated_skill.directory, &app) {
                log::warn!("Failed to sync the updated skill to {:?}: {e}", app);
            }
        }

        log::info!("Skill {} updated successfully", updated_skill.name);
        Ok(updated_skill)
    }

    /// Compute the missing content_hash of installed skills
    pub fn backfill_content_hashes(db: &Arc<Database>) -> Result<usize> {
        let _state_guard = skill_state_write_guard();
        let skills = db.get_all_installed_skills()?;
        let ssot_dir = Self::get_ssot_dir()?;
        let mut count = 0;

        for skill in skills.values() {
            if skill.content_hash.is_some() {
                continue;
            }
            let Ok(directory) = Self::require_valid_directory(&skill.directory) else {
                log::warn!(
                    "Skipping the hash backfill of an invalid directory: {:?}",
                    skill.directory
                );
                continue;
            };
            let skill_dir = ssot_dir.join(&directory);
            if !skill_dir.exists() {
                continue;
            }
            match Self::compute_dir_hash(&skill_dir) {
                Ok(hash) => {
                    let _ = db.update_skill_hash(&skill.id, &hash, 0);
                    count += 1;
                }
                Err(e) => {
                    log::warn!("Failed to compute the hash {}: {e}", skill.id);
                }
            }
        }

        if count > 0 {
            log::info!("Computed the content hash for {count} skills");
        }
        Ok(count)
    }

    /// Migrate the skill storage location (move the files between the two SSOT directories)
    ///
    /// Safety strategy: move the files first, change the setting afterwards. A crash in between leaves the setting pointing at the old directory.
    pub fn migrate_storage(
        db: &Arc<Database>,
        target: SkillStorageLocation,
    ) -> Result<MigrationResult> {
        let _state_guard = skill_state_write_guard();
        let current = crate::settings::get_skill_storage_location();
        if current == target {
            return Ok(MigrationResult {
                migrated_count: 0,
                skipped_count: 0,
                errors: vec![],
            });
        }

        // 1. Resolve the old and new directories (without changing the setting)
        let old_dir = Self::get_ssot_dir()?;
        let new_dir = match target {
            SkillStorageLocation::CcSwitch => {
                crate::infrastructure::paths::product_data_dir().join("skills")
            }
            SkillStorageLocation::Unified => {
                crate::config::get_home_dir().join(".agents").join("skills")
            }
        };
        fs::create_dir_all(&new_dir)?;
        Self::validate_skill_storage_destination(&new_dir)?;

        // 2. Move the skill directories one by one
        let skills = db.get_all_installed_skills()?;
        let pi_dir = Self::get_app_skills_dir(&AppType::Pi)?;
        let pi_uses_old_ssot = Self::paths_alias(&old_dir, &pi_dir);
        let mut pi_deployments = Vec::new();
        let mut pi_native_sources = Vec::new();
        let mut result = MigrationResult {
            migrated_count: 0,
            skipped_count: 0,
            errors: vec![],
        };

        for skill in skills.values() {
            // Below are rename and remove_dir_all, where a dirty directory could move or delete anything.
            // Soft failure: this function already collects errors, so record one and continue with the
            // other skills - do not abort everything, the user is only switching the storage location.
            let directory = match Self::require_valid_directory(&skill.directory) {
                Ok(directory) => directory,
                Err(err) => {
                    result
                        .errors
                        .push(format!("{}: {err}", skill.directory.escape_debug()));
                    continue;
                }
            };
            let src = old_dir.join(&directory);
            let dst = new_dir.join(&directory);

            if !src.exists() {
                result.skipped_count += 1;
                continue;
            }
            if dst.exists() {
                result.skipped_count += 1;
                continue;
            }

            // Before moving the SSOT, confirm the Pi target really is managed by the old source. An
            // external directory of the same name is neither migrated nor overwritten; a confirmed link or copy carries the old value into the protected replacement.
            let pi_deployment = if pi_uses_old_ssot {
                None
            } else {
                let pi_destination = pi_dir.join(&directory);
                Self::inspect_pi_skill_destination(&src, &pi_destination, &directory)
                    .ok()
                    .flatten()
            };

            // Prefer rename (atomic within one filesystem), falling back to copy+delete
            match fs::rename(&src, &dst) {
                Ok(()) => {
                    result.migrated_count += 1;
                    if pi_uses_old_ssot {
                        pi_native_sources.push(directory);
                    } else if let Some(deployment) = pi_deployment {
                        pi_deployments.push((directory, deployment));
                    }
                }
                Err(_) => match Self::copy_dir_recursive(&src, &dst) {
                    Ok(()) => {
                        let _ = fs::remove_dir_all(&src);
                        result.migrated_count += 1;
                        if pi_uses_old_ssot {
                            pi_native_sources.push(directory);
                        } else if let Some(deployment) = pi_deployment {
                            pi_deployments.push((directory, deployment));
                        }
                    }
                    Err(e) => {
                        result.errors.push(format!("{}: {e}", skill.directory));
                    }
                },
            }
        }

        // 3. Persist the setting only after the files have moved
        crate::settings::set_skill_storage_location(target)?;

        // 4. Refresh the symlinks of every app directory (pointing at the new SSOT)
        for app in AppType::all() {
            let _ = Self::sync_to_app_unlocked(db, &app);
        }
        for (directory, deployment) in pi_deployments {
            let source = new_dir.join(&directory);
            let destination = pi_dir.join(&directory);
            if let Err(err) =
                Self::refresh_pi_skill_destination(&source, &destination, &directory, &deployment)
            {
                result.errors.push(format!("{directory}: {err}"));
            }
        }
        for directory in pi_native_sources {
            if let Err(err) = Self::sync_to_app_dir(&directory, &AppType::Pi) {
                result.errors.push(format!("{directory}: {err}"));
            }
        }

        log::info!(
            "Skill storage migration finished: {} migrated, {} skipped, {} errors",
            result.migrated_count,
            result.skipped_count,
            result.errors.len()
        );

        Ok(result)
    }

    pub fn list_backups() -> Result<Vec<SkillBackupEntry>> {
        let backup_dir = Self::get_backup_dir()?;
        let mut entries = Vec::new();

        for entry in fs::read_dir(&backup_dir)? {
            let entry = match entry {
                Ok(entry) => entry,
                Err(err) => {
                    log::warn!("Failed to read a skill backup directory entry: {err}");
                    continue;
                }
            };
            let path = entry.path();
            if !path.is_dir() {
                continue;
            }

            match Self::read_backup_metadata(&path) {
                Ok(metadata) => entries.push(SkillBackupEntry {
                    backup_id: entry.file_name().to_string_lossy().to_string(),
                    backup_path: path.to_string_lossy().to_string(),
                    created_at: metadata.backup_created_at,
                    skill: metadata.skill,
                }),
                Err(err) => {
                    log::warn!(
                        "Failed to parse the skill backup {}: {err:#}",
                        path.display()
                    );
                }
            }
        }

        entries.sort_by_key(|entry| std::cmp::Reverse(entry.created_at));
        Ok(entries)
    }

    pub fn delete_backup(backup_id: &str) -> Result<()> {
        // Restore holds the same guard while it reads metadata and copies the
        // archived tree. Serializing deletion prevents a confirmed cleanup
        // from removing that tree halfway through a restore.
        let _state_guard = skill_state_write_guard();
        let backup_path = Self::backup_path_for_id(backup_id)?;
        let metadata = fs::symlink_metadata(&backup_path)
            .with_context(|| format!("failed to access {}", backup_path.display()))?;

        if !metadata.is_dir() {
            return Err(anyhow!(
                "Skill backup is not a directory: {}",
                backup_path.display()
            ));
        }

        fs::remove_dir_all(&backup_path)
            .with_context(|| format!("failed to delete {}", backup_path.display()))?;

        log::info!("Skill backup deleted: {}", backup_path.display());
        Ok(())
    }

    pub fn restore_from_backup(
        db: &Arc<Database>,
        backup_id: &str,
        current_app: &AppType,
    ) -> Result<InstalledSkill> {
        let _state_guard = skill_state_write_guard();
        let backup_path = Self::backup_path_for_id(backup_id)?;
        let metadata = Self::read_backup_metadata(&backup_path)?;
        let backup_skill_dir = backup_path.join("skill");
        if !backup_skill_dir.join("SKILL.md").exists() {
            return Err(anyhow!(
                "Skill backup is invalid or missing SKILL.md: {}",
                backup_path.display()
            ));
        }

        let existing_skills = db.get_all_installed_skills()?;
        if existing_skills.contains_key(&metadata.skill.id)
            || existing_skills.values().any(|skill| {
                skill
                    .directory
                    .eq_ignore_ascii_case(&metadata.skill.directory)
            })
        {
            return Err(anyhow!(
                "Skill already exists, please uninstall the current one first: {}",
                metadata.skill.directory
            ));
        }

        // meta.json is file content (possibly hand-placed or from an untrusted backup) and directory was
        // joined without any validation - it could escape the SSOT and write anywhere. Validate first.
        let directory = Self::require_valid_directory(&metadata.skill.directory)?;

        let ssot_dir = Self::get_ssot_dir()?;
        let restore_path = ssot_dir.join(&directory);
        if restore_path.exists() || Self::is_symlink(&restore_path) {
            return Err(anyhow!(
                "Restore target already exists: {}",
                restore_path.display()
            ));
        }

        let mut restored_skill = metadata.skill;
        restored_skill.directory = directory;
        restored_skill.installed_at = Utc::now().timestamp();
        restored_skill.apps = SkillApps::only(current_app);
        restored_skill.updated_at = 0;

        Self::copy_dir_recursive(&backup_skill_dir, &restore_path)?;

        // Recompute the content hash
        restored_skill.content_hash = Self::compute_dir_hash(&restore_path).ok();

        if let Err(err) = db.save_skill(&restored_skill) {
            let _ = fs::remove_dir_all(&restore_path);
            return Err(err.into());
        }

        if !restored_skill.apps.is_empty() {
            if let Err(err) = Self::sync_to_app_dir(&restored_skill.directory, current_app) {
                let _ = db.delete_skill(&restored_skill.id);
                let _ = fs::remove_dir_all(&restore_path);
                return Err(err);
            }
        }

        log::info!(
            "Skill {} restored from the backup into {}",
            restored_skill.name,
            restore_path.display()
        );

        Ok(restored_skill)
    }

    /// Toggle the enabled state for an app
    ///
    /// Enable: copy into the app directory
    /// Disable: delete from the app directory
    pub fn toggle_app(db: &Arc<Database>, id: &str, app: &AppType, enabled: bool) -> Result<()> {
        let _state_guard = skill_state_write_guard();
        // Read the current skill
        let mut skill = db
            .get_installed_skill(id)?
            .ok_or_else(|| anyhow!("Skill not found: {id}"))?;

        // Update the state
        skill.apps.set_enabled_for(app, enabled);

        // Sync the files
        if enabled {
            Self::sync_to_app_dir(&skill.directory, app)?;
        } else {
            Self::remove_from_app(&skill.directory, app)?;
        }

        // Pi follows its native exists=active rule; other apps keep their
        // established persisted desired-state flags.
        if !matches!(app, AppType::Pi) {
            db.update_skill_apps(id, &skill.apps)?;
        }

        log::info!(
            "The {:?} state of skill {} was updated to {}",
            app,
            skill.name,
            enabled
        );

        Ok(())
    }

    /// Scan for unmanaged skills
    ///
    /// Scans every app directory for skills not managed by CC Switch
    pub fn scan_unmanaged(db: &Arc<Database>) -> Result<Vec<UnmanagedSkill>> {
        let _state_guard = skill_state_read_guard();
        let managed_skills = db.get_all_installed_skills()?;
        let managed_dirs: HashSet<String> = managed_skills
            .values()
            .map(|s| s.directory.clone())
            .collect();

        // Collect every directory to scan and its source label
        let mut scan_sources: Vec<(PathBuf, String)> = Vec::new();
        for app in AppType::all() {
            if let Ok(d) = Self::get_app_skills_dir(&app) {
                scan_sources.push((d, app.as_str().to_string()));
            }
        }
        if let Some(agents_dir) = get_agents_skills_dir() {
            scan_sources.push((agents_dir, "agents".to_string()));
        }
        if let Ok(ssot_dir) = Self::get_ssot_dir() {
            scan_sources.push((ssot_dir, "cc-switch".to_string()));
        }

        let mut unmanaged: HashMap<String, UnmanagedSkill> = HashMap::new();

        for (scan_dir, label) in &scan_sources {
            let entries = match fs::read_dir(scan_dir) {
                Ok(e) => e,
                Err(_) => continue,
            };
            for entry in entries.flatten() {
                let path = entry.path();
                if !path.is_dir() {
                    continue;
                }
                let dir_name = entry.file_name().to_string_lossy().to_string();
                if dir_name.starts_with('.') || managed_dirs.contains(&dir_name) {
                    continue;
                }

                let skill_md = path.join("SKILL.md");
                if !skill_md.exists() {
                    continue;
                }
                let (name, description) = Self::read_skill_name_desc(&skill_md, &dir_name);

                unmanaged
                    .entry(dir_name.clone())
                    .and_modify(|s| s.found_in.push(label.clone()))
                    .or_insert(UnmanagedSkill {
                        directory: dir_name,
                        name,
                        description,
                        found_in: vec![label.clone()],
                        path: path.display().to_string(),
                    });
            }
        }

        Ok(unmanaged.into_values().collect())
    }

    /// Import skills from the app directories
    ///
    /// Brings unmanaged skills under unified CC Switch management
    pub fn import_from_apps(
        db: &Arc<Database>,
        imports: Vec<ImportSkillSelection>,
    ) -> Result<Vec<InstalledSkill>> {
        let _state_guard = skill_state_write_guard();
        let ssot_dir = Self::get_ssot_dir()?;
        let agents_lock = parse_agents_lock();
        let mut imported = Vec::new();

        // Save the repositories discovered in the lock file into skill_repos
        save_repos_from_lock(
            db,
            &agents_lock,
            imports.iter().map(|selection| selection.directory.as_str()),
        );

        // Collect every candidate search directory
        let mut search_sources: Vec<(PathBuf, String)> = Vec::new();
        for app in AppType::all() {
            if let Ok(d) = Self::get_app_skills_dir(&app) {
                search_sources.push((d, app.as_str().to_string()));
            }
        }
        if let Some(agents_dir) = get_agents_skills_dir() {
            search_sources.push((agents_dir, "agents".to_string()));
        }
        search_sources.push((ssot_dir.clone(), "cc-switch".to_string()));

        for selection in imports {
            // selection.directory comes straight from the frontend IPC with no prior validation, and it
            // is used to probe the source directory, as the copy_dir_recursive target, and finally stored
            // verbatim. Rejecting it at the entry point cuts off both the "dirty sink" and the "dirty source".
            let dir_name = match Self::require_valid_directory(&selection.directory) {
                Ok(dir_name) => dir_name,
                Err(err) => {
                    log::warn!("Skipping the import: {err}");
                    continue;
                }
            };
            // Search every candidate directory
            let mut source_path: Option<PathBuf> = None;

            for (base, label) in &search_sources {
                let skill_path = base.join(&dir_name);
                if skill_path.exists() {
                    if source_path.is_none() {
                        source_path = Some(skill_path);
                    }
                    log::debug!("Skill '{dir_name}' found in source '{label}'");
                }
            }

            let source = match source_path {
                Some(p) => p,
                None => continue,
            };
            if !source.join("SKILL.md").exists() {
                log::warn!(
                    "Skip importing '{}' because source '{}' has no SKILL.md",
                    dir_name,
                    source.display()
                );
                continue;
            }

            // Copy into the SSOT
            let dest = ssot_dir.join(&dir_name);
            if !dest.exists() {
                Self::copy_dir_recursive(&source, &dest)?;
            }

            // Parse the metadata
            let skill_md = dest.join("SKILL.md");
            let (name, description) = Self::read_skill_name_desc(&skill_md, &dir_name);

            // Other apps keep the user's selection; the exists=active of Pi must come from the native directory.
            let mut apps = selection.apps;
            apps.pi = Self::skill_exists_in_app(&dir_name, &AppType::Pi);

            // Extract the repository info from the lock file
            let (id, repo_owner, repo_name, repo_branch, readme_url) =
                build_repo_info_from_lock(&agents_lock, &dir_name);

            // Compute the content hash
            let ssot_skill_dir = ssot_dir.join(&dir_name);
            let content_hash = Self::compute_dir_hash(&ssot_skill_dir).ok();

            // Create the record
            let skill = InstalledSkill {
                id,
                name,
                description,
                directory: dir_name,
                repo_owner,
                repo_name,
                repo_branch,
                readme_url,
                apps,
                installed_at: chrono::Utc::now().timestamp(),
                content_hash,
                updated_at: 0,
            };

            // Save to the database
            db.save_skill(&skill)?;

            imported.push(skill);
        }

        log::info!("Imported {} skills successfully", imported.len());

        Ok(imported)
    }

    // ========== File sync methods ==========

    /// Create a symbolic link (cross-platform)
    ///
    /// - Unix: uses std::os::unix::fs::symlink
    /// - Windows: uses std::os::windows::fs::symlink_dir
    #[cfg(unix)]
    fn create_symlink(src: &Path, dest: &Path) -> Result<()> {
        std::os::unix::fs::symlink(src, dest).with_context(|| {
            format!(
                "Failed to create the symbolic link: {} -> {}",
                src.display(),
                dest.display()
            )
        })
    }

    #[cfg(windows)]
    fn create_symlink(src: &Path, dest: &Path) -> Result<()> {
        std::os::windows::fs::symlink_dir(src, dest).with_context(|| {
            format!(
                "Failed to create the symbolic link: {} -> {}",
                src.display(),
                dest.display()
            )
        })
    }

    /// Check whether a path is a symbolic link
    fn is_symlink(path: &Path) -> bool {
        path.symlink_metadata()
            .map(|m| m.file_type().is_symlink())
            .unwrap_or(false)
    }

    fn skill_exists_in_app(directory: &str, app: &AppType) -> bool {
        let Ok(directory) = Self::require_valid_directory(directory) else {
            return false;
        };
        let Ok(app_dir) = Self::get_app_skills_dir(app) else {
            return false;
        };
        app_dir.join(directory).is_dir()
    }

    fn preflight_install_destination(source: &Path, directory: &str, app: &AppType) -> Result<()> {
        let ssot_dir = Self::get_ssot_dir()?;
        let app_dir = Self::get_distinct_app_skills_dir(&ssot_dir, app)?;
        if !matches!(app, AppType::Pi) {
            return Ok(());
        }
        let destination = app_dir.join(directory);
        if destination.exists() || Self::is_symlink(&destination) {
            Self::ensure_pi_skill_destination_matches(source, &destination, directory)?;
        }
        Ok(())
    }

    fn persist_and_sync_new_skill(
        db: &Arc<Database>,
        skill: &InstalledSkill,
        app: &AppType,
    ) -> Result<()> {
        let source = Self::get_ssot_dir()?.join(&skill.directory);
        Self::preflight_install_destination(&source, &skill.directory, app)?;
        db.save_skill(skill)?;
        if let Err(error) = Self::sync_to_app_dir(&skill.directory, app) {
            if let Err(rollback_error) = db.delete_skill(&skill.id) {
                log::error!(
                    "Failed to roll back Skill {} after sync error: {rollback_error}",
                    skill.id
                );
            }
            return Err(error);
        }
        Ok(())
    }

    fn ensure_pi_skill_destination_matches(
        source: &Path,
        destination: &Path,
        directory: &str,
    ) -> Result<()> {
        Self::inspect_pi_skill_destination(source, destination, directory).map(|_| ())
    }

    fn inspect_pi_skill_destination(
        source: &Path,
        destination: &Path,
        directory: &str,
    ) -> Result<Option<PiSkillDeployment>> {
        if !destination.exists() && !Self::is_symlink(destination) {
            return Ok(None);
        }

        if Self::is_symlink(destination) {
            let target = fs::read_link(destination)?;
            let resolved = if target.is_absolute() {
                target
            } else {
                destination
                    .parent()
                    .map(|parent| parent.join(&target))
                    .unwrap_or(target)
            };
            if matches!(
                (resolved.canonicalize(), source.canonicalize()),
                (Ok(resolved), Ok(source)) if resolved == source
            ) {
                return Ok(Some(PiSkillDeployment::Symlink {
                    expected_target: resolved,
                }));
            }
        } else if destination.is_dir() {
            if let (Ok(destination_hash), Ok(source_hash)) = (
                Self::compute_pi_deployment_hash(destination),
                Self::compute_pi_deployment_hash(source),
            ) {
                if destination_hash == source_hash {
                    return Ok(Some(PiSkillDeployment::Copy {
                        expected_hash: destination_hash,
                    }));
                }
            }
        }

        Err(anyhow!(
            "A skill with the same name but different content already exists in Pi, refusing to overwrite or delete it: {directory}"
        ))
    }

    fn remove_verified_pi_destination(
        source: &Path,
        destination: &Path,
        directory: &str,
    ) -> Result<()> {
        match Self::inspect_pi_skill_destination(source, destination, directory)? {
            Some(_) => Self::remove_path(destination),
            None => Ok(()),
        }
    }

    fn refresh_pi_skill_destination(
        source: &Path,
        destination: &Path,
        directory: &str,
        deployment: &PiSkillDeployment,
    ) -> Result<()> {
        Self::validate_sync_source_dir(source, directory)?;

        match deployment {
            PiSkillDeployment::Symlink { expected_target } => {
                if !Self::is_symlink(destination) {
                    return Err(anyhow!(
                        "The skill in Pi changed during the operation, refusing to overwrite it: {directory}"
                    ));
                }
                let target = fs::read_link(destination)?;
                let resolved = if target.is_absolute() {
                    target
                } else {
                    destination
                        .parent()
                        .map(|parent| parent.join(&target))
                        .unwrap_or(target)
                };
                if &resolved != expected_target {
                    return Err(anyhow!(
                        "The skill in Pi changed during the operation, refusing to overwrite it: {directory}"
                    ));
                }
                Self::remove_path(destination)?;
                Self::create_symlink(source, destination)?;
            }
            PiSkillDeployment::Copy { expected_hash } => {
                if Self::is_symlink(destination)
                    || !destination.is_dir()
                    || !matches!(
                        Self::compute_pi_deployment_hash(destination),
                        Ok(current_hash) if &current_hash == expected_hash
                    )
                {
                    return Err(anyhow!(
                        "The skill in Pi changed during the operation, refusing to overwrite it: {directory}"
                    ));
                }
                Self::replace_dest_with_copy(source, destination, directory, true)?;
            }
        }

        Ok(())
    }

    /// Get the current sync method setting
    fn get_sync_method() -> SyncMethod {
        crate::settings::get_skill_sync_method()
    }

    /// Sync a skill into an app directory (using symlink or copy)
    ///
    /// Picks the best sync method based on the setting and the platform:
    /// - Auto: try symlink first, fall back to copy on failure
    /// - Symlink: symlink only
    /// - Copy: file copy only
    pub fn sync_to_app_dir(directory: &str, app: &AppType) -> Result<()> {
        if matches!(app, AppType::ClaudeDesktop) {
            return Ok(());
        }

        // directory may come from a polluted DB row (e.g. a sync-imported remote snapshot), so validate before joining.
        let directory = Self::require_valid_directory(directory)?;

        let ssot_dir = Self::get_ssot_dir()?;
        let source = ssot_dir.join(&directory);

        Self::validate_sync_source_dir(&source, &directory)?;

        let app_dir = Self::get_distinct_app_skills_dir(&ssot_dir, app)?;
        fs::create_dir_all(&app_dir)?;

        let dest = app_dir.join(&directory);

        if matches!(app, AppType::Pi) && (dest.exists() || Self::is_symlink(&dest)) {
            Self::ensure_pi_skill_destination_matches(&source, &dest, &directory)?;
        }

        let sync_method = Self::get_sync_method();

        match sync_method {
            SyncMethod::Auto => {
                if dest.exists() && !Self::is_symlink(&dest) {
                    Self::replace_dest_with_copy(&source, &dest, &directory, true)?;
                    log::debug!("Skill {directory} was synced to {app:?} by copying");
                    return Ok(());
                }

                if Self::is_symlink(&dest) {
                    Self::remove_path(&dest)?;
                }

                // Try symlink first
                match Self::create_symlink(&source, &dest) {
                    Ok(()) => {
                        log::debug!("Skill {directory} was synced to {app:?} via symlink");
                        return Ok(());
                    }
                    Err(err) => {
                        log::warn!(
                            "Symlink creation failed, falling back to a file copy: {} -> {}. Error: {err:#}",
                            source.display(),
                            dest.display()
                        );
                    }
                }
                // Fall back to copy
                Self::replace_dest_with_copy(&source, &dest, &directory, true)?;
                log::debug!("Skill {directory} was synced to {app:?} by copying");
            }
            SyncMethod::Symlink => {
                if dest.exists() || Self::is_symlink(&dest) {
                    Self::remove_path(&dest)?;
                }
                Self::create_symlink(&source, &dest)?;
                log::debug!("Skill {directory} was synced to {app:?} via symlink");
            }
            SyncMethod::Copy => {
                Self::replace_dest_with_copy(&source, &dest, &directory, true)?;
                log::debug!("Skill {directory} was synced to {app:?} by copying");
            }
        }

        Ok(())
    }

    /// Copy a skill directory that AI Manager does not own from where it was discovered into a tool's
    /// skills directory.
    ///
    /// There is only one difference from `sync_to_app_dir`, but it matters: that function hardcodes the
    /// source as `get_ssot_dir()?.join(directory)`, while a detected skill usually only exists in
    /// `~/.claude/skills/<dir>` or `~/.agents/skills/<dir>`. The source path is re-resolved by the caller
    /// from the local manifest and never comes from the renderer.
    ///
    /// Always a real copy, regardless of `SyncMethod`: a symlink would make the target tool follow a
    /// directory this product does not own, and renaming or deleting the source would silently empty it.
    ///
    /// An existing entry with the same name at the target is always an error. It could be the user's own
    /// skill of the same name or a managed AI Manager copy; overwriting would silently destroy data. The
    /// UI already greys out tools that "already have it", and this layer is the backstop.
    pub fn copy_detected_to_app_dir(source: &Path, directory: &str, app: &AppType) -> Result<()> {
        let directory = Self::require_valid_directory(directory)?;
        Self::validate_sync_source_dir(source, &directory)?;

        let ssot_dir = Self::get_ssot_dir()?;
        let app_dir = Self::get_distinct_app_skills_dir(&ssot_dir, app)?;
        let dest = app_dir.join(&directory);

        // Source and target are the same place: this tool is where it was discovered, nothing to do.
        if Self::paths_alias(source, &dest) {
            return Ok(());
        }

        if dest.exists() || Self::is_symlink(&dest) {
            return Err(anyhow!(
                "The {app:?} skills directory already contains {directory}, refusing to overwrite it"
            ));
        }

        fs::create_dir_all(&app_dir)?;
        Self::replace_dest_with_copy(source, &dest, &directory, false)
    }

    /// Copy a skill into an app directory (kept for backward compatibility)
    #[deprecated(note = "use sync_to_app_dir() instead")]
    pub fn copy_to_app(directory: &str, app: &AppType) -> Result<()> {
        Self::sync_to_app_dir(directory, app)
    }

    /// Delete a path (handles both symlinks and real directories)
    fn remove_path(path: &Path) -> Result<()> {
        if Self::is_symlink(path) {
            // Symbolic link: delete only the link, leaving the source untouched
            #[cfg(unix)]
            fs::remove_file(path)?;
            #[cfg(windows)]
            fs::remove_dir(path)?; // directory symlinks on Windows need remove_dir
        } else if path.is_dir() {
            // Real directory: delete recursively
            fs::remove_dir_all(path)?;
        } else if path.exists() {
            // Plain file
            fs::remove_file(path)?;
        }
        Ok(())
    }

    fn validate_sync_source_dir(source: &Path, directory: &str) -> Result<()> {
        if !source.is_dir() {
            return Err(anyhow!("Skill not present in the SSOT: {directory}"));
        }

        let manifest = source.join("SKILL.md");
        if !manifest.is_file() {
            return Err(anyhow!(
                "The skill source directory has no SKILL.md, refusing to sync to avoid overwriting the target directory: {}",
                source.display()
            ));
        }

        Ok(())
    }

    /// `follow_links` refers to how entries **inside the source directory** are handled. The SSOT this
    /// product writes itself never contains third-party symlinks, so following them is safe; third-party
    /// directories are not, see `copy_dir_recursive_without_links`.
    fn replace_dest_with_copy(
        source: &Path,
        dest: &Path,
        directory: &str,
        follow_links: bool,
    ) -> Result<()> {
        Self::validate_sync_source_dir(source, directory)?;

        let parent = dest
            .parent()
            .ok_or_else(|| anyhow!("Invalid skill destination: {}", dest.display()))?;
        fs::create_dir_all(parent)?;

        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos();
        let tmp_name = Self::sanitize_backup_segment(directory);
        let tmp = parent.join(format!(".{tmp_name}.tmp-{}-{nonce}", std::process::id()));

        if tmp.exists() || Self::is_symlink(&tmp) {
            Self::remove_path(&tmp)?;
        }

        let copy_result = if follow_links {
            Self::copy_dir_recursive(source, &tmp)
        } else {
            Self::copy_dir_recursive_without_links(source, &tmp)
        };
        if let Err(err) = copy_result {
            let _ = Self::remove_path(&tmp);
            return Err(err);
        }

        if dest.exists() || Self::is_symlink(dest) {
            Self::remove_path(dest)?;
        }

        fs::rename(&tmp, dest).with_context(|| {
            let _ = Self::remove_path(&tmp);
            format!(
                "Failed to replace the skill directory: {} -> {}",
                tmp.display(),
                dest.display()
            )
        })?;

        Ok(())
    }

    /// Decide whether a path is a symbolic link pointing inside the SSOT directory.
    fn is_symlink_to_ssot(path: &Path, ssot_dir: &Path) -> bool {
        if !Self::is_symlink(path) {
            return false;
        }

        let Ok(target) = fs::read_link(path) else {
            return false;
        };

        if target.is_absolute() && target.starts_with(ssot_dir) {
            return true;
        }

        let resolved = path
            .parent()
            .map(|parent| parent.join(&target))
            .unwrap_or(target.clone());

        let canonical_ssot = ssot_dir
            .canonicalize()
            .unwrap_or_else(|_| ssot_dir.to_path_buf());
        let canonical_target = resolved.canonicalize().unwrap_or(resolved);

        canonical_target.starts_with(&canonical_ssot)
    }

    /// Delete a skill from an app directory (handles both symlinks and real directories)
    pub fn remove_from_app(directory: &str, app: &AppType) -> Result<()> {
        Self::remove_from_app_preserving(directory, app, None)
    }

    fn remove_from_app_preserving(
        directory: &str,
        app: &AppType,
        preserved_path: Option<&Path>,
    ) -> Result<()> {
        if matches!(app, AppType::ClaudeDesktop) {
            return Ok(());
        }

        // directory may come from a polluted DB row (e.g. a sync-imported remote snapshot) and this is a
        // delete operation, so it must be validated before joining to prevent deleting an arbitrary directory.
        let directory = Self::require_valid_directory(directory)?;

        let ssot_dir = Self::get_ssot_dir()?;
        let app_dir = Self::get_distinct_app_skills_dir(&ssot_dir, app)?;
        let skill_path = app_dir.join(&directory);

        if preserved_path.is_some_and(|path| Self::paths_overlap(&skill_path, path)) {
            log::debug!("The {app:?} path of skill {directory} equals the preserved Pi copy, skipping the deletion");
            return Ok(());
        }

        if skill_path.exists() || Self::is_symlink(&skill_path) {
            if matches!(app, AppType::Pi) {
                let source = ssot_dir.join(&directory);
                Self::ensure_pi_skill_destination_matches(&source, &skill_path, &directory)?;
            }
            Self::remove_path(&skill_path)?;
            log::debug!("Skill {directory} was deleted from {app:?}");
        }

        Ok(())
    }

    /// Sync every enabled skill to a given app
    pub fn sync_to_app(db: &Arc<Database>, app: &AppType) -> Result<()> {
        let _state_guard = skill_state_read_guard();
        Self::sync_to_app_unlocked(db, app)
    }

    /// Caller must hold either the Skills state read or write guard.
    fn sync_to_app_unlocked(db: &Arc<Database>, app: &AppType) -> Result<()> {
        if matches!(app, AppType::ClaudeDesktop | AppType::Pi) {
            return Ok(());
        }

        let skills = db.get_all_installed_skills()?;
        let ssot_dir = Self::get_ssot_dir()?;
        let app_dir = Self::get_distinct_app_skills_dir(&ssot_dir, app)?;

        let indexed_skills: HashMap<String, &InstalledSkill> = skills
            .values()
            .map(|skill| (skill.directory.to_lowercase(), skill))
            .collect();

        if app_dir.exists() {
            for entry in fs::read_dir(&app_dir)? {
                let entry = entry?;
                let path = entry.path();
                let dir_name = entry.file_name().to_string_lossy().to_string();

                if dir_name.starts_with('.') {
                    continue;
                }

                if let Some(skill) = indexed_skills.get(&dir_name.to_lowercase()) {
                    if !skill.apps.is_enabled_for(app) {
                        Self::remove_path(&path)?;
                    }
                    continue;
                }

                if Self::is_symlink_to_ssot(&path, &ssot_dir) {
                    Self::remove_path(&path)?;
                }
            }
        }

        for skill in skills.values() {
            if skill.apps.is_enabled_for(app) {
                // Per-item fault tolerance instead of propagating with `?`: this function runs during a
                // provider switch, and one dirty directory (a legacy dot directory, or a row pushed in by
                // a sync import) must not break the skill sync of the whole app.
                if let Err(err) = Self::sync_to_app_dir(&skill.directory, app) {
                    log::warn!(
                        "Failed to sync skill {} to {app:?}, skipping it: {err}",
                        skill.directory
                    );
                }
            }
        }

        Ok(())
    }

    // ========== Discovery (existing logic kept) ==========

    /// List every discoverable skill (fetched from the repositories)
    pub async fn discover_available(
        &self,
        repos: Vec<SkillRepo>,
    ) -> Result<Vec<DiscoverableSkill>> {
        let mut skills = Vec::new();

        // Only use enabled repositories
        let enabled_repos: Vec<SkillRepo> = repos.into_iter().filter(|repo| repo.enabled).collect();

        let enabled_count = enabled_repos.len();
        let fetch_tasks = enabled_repos
            .iter()
            .map(|repo| self.fetch_repo_skills(repo));

        let results: Vec<Result<Vec<DiscoverableSkill>>> =
            futures::future::join_all(fetch_tasks).await;

        let mut successful_sources = 0usize;
        let mut last_error = None;
        for (repo, result) in enabled_repos.into_iter().zip(results) {
            match result {
                Ok(repo_skills) => {
                    successful_sources += 1;
                    skills.extend(repo_skills);
                }
                Err(error) => {
                    log::warn!(
                        "Failed to fetch skills of repository {}/{}: {}",
                        repo.owner,
                        repo.name,
                        error
                    );
                    last_error = Some(error);
                }
            }
        }

        // The original implementation swallowed every network error and returned an empty success array,
        // so the renderer could only show "no skills" without a retry/proxy hint. Errors are only raised
        // when every enabled source failed; a partial catalog is kept when at least one source succeeded.
        if enabled_count > 0 && successful_sources == 0 {
            return Err(last_error.unwrap_or_else(|| {
                anyhow!(format_skill_error(
                    "DOWNLOAD_FAILED",
                    &[("status", "all_sources_unavailable")],
                    Some("checkNetwork"),
                ))
            }));
        }

        // Deduplicate and sort
        Self::deduplicate_discoverable_skills(&mut skills);
        skills.sort_by_key(|skill| skill.name.to_lowercase());

        Ok(skills)
    }

    /// List every skill (legacy API compatibility)
    pub async fn list_skills(
        &self,
        repos: Vec<SkillRepo>,
        db: &Arc<Database>,
    ) -> Result<Vec<Skill>> {
        // Get the discoverable skills
        let discoverable = self.discover_available(repos).await?;

        // Get the installed skills
        let installed = db.get_all_installed_skills()?;
        let installed_dirs: HashSet<String> =
            installed.values().map(|s| s.directory.clone()).collect();

        // Convert into the Skill format
        let mut skills: Vec<Skill> = discoverable
            .into_iter()
            .map(|d| {
                let install_name = Path::new(&d.directory)
                    .file_name()
                    .map(|s| s.to_string_lossy().to_string())
                    .unwrap_or_else(|| d.directory.clone());

                Skill {
                    key: d.key,
                    name: d.name,
                    description: d.description,
                    directory: d.directory,
                    readme_url: d.readme_url,
                    installed: installed_dirs.contains(&install_name),
                    repo_owner: Some(d.repo_owner),
                    repo_name: Some(d.repo_name),
                    repo_branch: Some(d.repo_branch),
                    mirror_used: d.mirror_used,
                }
            })
            .collect();

        // Add skills installed locally but absent from the repositories
        for skill in installed.values() {
            let already_in_list = skills.iter().any(|s| {
                let s_install_name = Path::new(&s.directory)
                    .file_name()
                    .map(|n| n.to_string_lossy().to_string())
                    .unwrap_or_else(|| s.directory.clone());
                s_install_name == skill.directory
            });

            if !already_in_list {
                skills.push(Skill {
                    key: skill.id.clone(),
                    name: skill.name.clone(),
                    description: skill.description.clone().unwrap_or_default(),
                    directory: skill.directory.clone(),
                    readme_url: skill.readme_url.clone(),
                    installed: true,
                    repo_owner: skill.repo_owner.clone(),
                    repo_name: skill.repo_name.clone(),
                    repo_branch: skill.repo_branch.clone(),
                    mirror_used: false,
                });
            }
        }

        skills.sort_by_key(|skill| skill.name.to_lowercase());

        Ok(skills)
    }

    /// Fetch the skill list from a repository
    async fn fetch_repo_skills(&self, repo: &SkillRepo) -> Result<Vec<DiscoverableSkill>> {
        let (temp_guard, resolved_branch, delivery) =
            timeout(std::time::Duration::from_secs(60), self.download_repo(repo))
                .await
                .map_err(|_| {
                    anyhow!(format_skill_error(
                        "DOWNLOAD_TIMEOUT",
                        &[
                            ("owner", &repo.owner),
                            ("name", &repo.name),
                            ("timeout", "60")
                        ],
                        Some("checkNetwork"),
                    ))
                })??;

        let mut skills = Vec::new();
        let scan_dir = temp_guard.path();
        let mut resolved_repo = repo.clone();
        resolved_repo.branch = resolved_branch;
        self.scan_dir_recursive(scan_dir, scan_dir, &resolved_repo, &mut skills)?;
        let mirror_used = delivery == SkillRepoDelivery::Jsdelivr;
        for skill in &mut skills {
            skill.mirror_used = mirror_used;
        }

        Ok(skills)
    }

    /// Recursively scan a directory for SKILL.md
    pub(crate) fn scan_dir_recursive(
        &self,
        current_dir: &Path,
        base_dir: &Path,
        repo: &SkillRepo,
        skills: &mut Vec<DiscoverableSkill>,
    ) -> Result<()> {
        let skill_md = current_dir.join("SKILL.md");

        if skill_md.exists() {
            let directory = if current_dir == base_dir {
                repo.name.clone()
            } else {
                current_dir
                    .strip_prefix(base_dir)
                    .unwrap_or(current_dir)
                    .to_string_lossy()
                    .replace('\\', "/")
            };

            let doc_path = skill_md
                .strip_prefix(base_dir)
                .unwrap_or(skill_md.as_path())
                .to_string_lossy()
                .replace('\\', "/");

            if let Ok(skill) =
                self.build_skill_from_metadata(&skill_md, &directory, &doc_path, repo)
            {
                skills.push(skill);
            }

            return Ok(());
        }

        for entry in fs::read_dir(current_dir)? {
            let entry = entry?;
            let path = entry.path();

            if path.is_dir() {
                self.scan_dir_recursive(&path, base_dir, repo, skills)?;
            }
        }

        Ok(())
    }

    /// Build a skill object from SKILL.md
    fn build_skill_from_metadata(
        &self,
        skill_md: &Path,
        directory: &str,
        doc_path: &str,
        repo: &SkillRepo,
    ) -> Result<DiscoverableSkill> {
        let meta = self.parse_skill_metadata(skill_md)?;

        Ok(DiscoverableSkill {
            key: format!("{}/{}:{}", repo.owner, repo.name, directory),
            name: meta.name.unwrap_or_else(|| directory.to_string()),
            description: meta.description.unwrap_or_default(),
            directory: directory.to_string(),
            readme_url: Self::build_skill_doc_url(&repo.owner, &repo.name, &repo.branch, doc_path),
            repo_owner: repo.owner.clone(),
            repo_name: repo.name.clone(),
            repo_branch: repo.branch.clone(),
            mirror_used: false,
        })
    }

    /// Parse the skill metadata
    fn parse_skill_metadata(&self, path: &Path) -> Result<SkillMetadata> {
        Self::parse_skill_metadata_static(path)
    }

    /// Static method: parse the skill metadata
    fn parse_skill_metadata_static(path: &Path) -> Result<SkillMetadata> {
        let content = fs::read_to_string(path)?;
        let content = content.trim_start_matches('\u{feff}');

        let parts: Vec<&str> = content.splitn(3, "---").collect();
        if parts.len() < 3 {
            return Ok(SkillMetadata {
                name: None,
                description: None,
            });
        }

        let front_matter = parts[1].trim();
        let meta: SkillMetadata = serde_yaml::from_str(front_matter).unwrap_or(SkillMetadata {
            name: None,
            description: None,
        });

        Ok(meta)
    }

    /// Read the name and description from SKILL.md, falling back to the directory name
    fn read_skill_name_desc(skill_md: &Path, fallback_name: &str) -> (String, Option<String>) {
        if skill_md.exists() {
            match Self::parse_skill_metadata_static(skill_md) {
                Ok(meta) => (
                    meta.name.unwrap_or_else(|| fallback_name.to_string()),
                    meta.description,
                ),
                Err(_) => (fallback_name.to_string(), None),
            }
        } else {
            (fallback_name.to_string(), None)
        }
    }

    /// Validate and normalize a skill source path (multi-level directories allowed), rejecting traversal and absolute paths
    fn sanitize_skill_source_path(raw: &str) -> Option<PathBuf> {
        let trimmed = raw.trim();
        if trimmed.is_empty() {
            return None;
        }

        let mut normalized = PathBuf::new();
        let mut has_component = false;

        for component in Path::new(trimmed).components() {
            match component {
                Component::Normal(name) => {
                    let segment = name.to_string_lossy().trim().to_string();
                    if segment.is_empty() || segment == "." || segment == ".." {
                        return None;
                    }
                    normalized.push(segment);
                    has_component = true;
                }
                Component::CurDir
                | Component::ParentDir
                | Component::RootDir
                | Component::Prefix(_) => {
                    return None;
                }
            }
        }

        has_component.then_some(normalized)
    }

    /// Validate and normalize the install directory name (the final on-disk name, a single segment only)
    fn sanitize_install_name(raw: &str) -> Option<String> {
        let trimmed = raw.trim();
        if trimmed.is_empty() {
            return None;
        }

        // Both separators are rejected explicitly, as the platform semantics of components() cannot be
        // relied on: `\` is not a separator on Linux/macOS and would pass as a valid single segment,
        // but the same value becomes a nested path once synced/restored on Windows.
        if trimmed.contains('/') || trimmed.contains('\\') {
            return None;
        }

        let path = Path::new(trimmed);
        let mut components = path.components();
        match (components.next(), components.next()) {
            (Some(Component::Normal(name)), None) => {
                let normalized = name.to_string_lossy().trim().to_string();
                if normalized.is_empty()
                    || normalized == "."
                    || normalized == ".."
                    || normalized.starts_with('.')
                {
                    None
                } else {
                    Some(normalized)
                }
            }
            _ => None,
        }
    }

    /// Validate the directory field coming from external sources such as DB rows / backup meta.json.
    ///
    /// By construction the stored value should be a single-segment install name (see
    /// sanitize_install_name), but two entry points bypass the install-time validation: a sync-imported
    /// remote snapshot written with raw SQL, and the meta.json of a hand-placed / untrusted backup. Every
    /// use that joins it into a filesystem path (especially deletes such as remove_dir_all) must pass this check first.
    ///
    /// Validation only, no normalization: `sanitize_install_name` calls `trim()`, and replacing the
    /// original value with its result would make a directory name that really contains spaces
    /// unreachable. The normalized result must therefore be byte-identical, or the value is rejected.
    fn require_valid_directory(directory: &str) -> Result<String> {
        match Self::sanitize_install_name(directory) {
            Some(normalized) if normalized == directory => Ok(normalized),
            _ => Err(anyhow!(
                "Invalid skill directory (possible path traversal): {directory:?}"
            )),
        }
    }

    /// GitHub account name (user / org login).
    ///
    /// Only ASCII alphanumerics and `-` are allowed. This is stricter than GitHub itself, but the field
    /// is spliced into the download URL where any `/`, `.`, `%` or `\` could redirect the request (see validate_repo_ref).
    fn is_valid_github_owner(owner: &str) -> bool {
        !owner.is_empty()
            && owner.len() <= 39
            && owner.chars().all(|c| c.is_ascii_alphanumeric() || c == '-')
    }

    /// GitHub repository name. Allows `.` `-` `_`, but the whole name cannot be `.` or `..`.
    fn is_valid_github_repo_name(name: &str) -> bool {
        !name.is_empty()
            && name.len() <= 100
            && name != "."
            && name != ".."
            && name
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '-' | '_'))
    }

    /// Git branch name.
    ///
    /// A branch name legitimately contains `/` (`feature/x`), so separators cannot be banned outright -
    /// the allowlist is applied per segment. Per-segment `!starts_with('.')` is sturdier than a global
    /// `contains("..")`: it also blocks variants such as `a/./b` and `a/.../b`. On top of the
    /// `git check-ref-format` rules, `#` and `%` are also banned: the former turns the rest of the URL
    /// into a fragment, the latter can bypass the character check via percent encoding.
    fn is_valid_git_branch(branch: &str) -> bool {
        // The empty string and "HEAD" are both sentinels of `download_repo` meaning "use the repository
        // default branch": the branch candidate table skips both and tries main / master instead, so they
        // **never reach a URL** and have no attack surface to validate. The empty string must be allowed -
        // existing `skill_repos` rows may have an empty branch (the column default is 'main' but the empty
        // string is not forbidden), and the two frontend `repo.branch || "main"` expressions assume this.
        // Treating it as invalid would make download_repo fail with INVALID_REPO_REF on its first line and hide those repositories from the skill panel.
        if branch.is_empty() || branch.eq_ignore_ascii_case("HEAD") {
            return true;
        }
        if branch.len() > 255 {
            return false;
        }
        if branch.starts_with('/') || branch.ends_with('/') || branch.contains("//") {
            return false;
        }
        if branch.contains("@{") {
            return false;
        }
        // The range of `is_ascii_control()` is U+0000..=U+001F **plus** U+007F DELETE,
        // so DEL does not need to be listed separately.
        if branch
            .chars()
            .any(|c| c.is_ascii_control() || " ~^:?*[\\#%".contains(c))
        {
            return false;
        }
        branch.split('/').all(|segment| {
            !segment.is_empty()
                && !segment.starts_with('.')
                && !segment.ends_with('.')
                && !segment.ends_with(".lock")
        })
    }

    /// Validate a set of repository coordinates, for anywhere they get spliced into a github.com URL.
    ///
    /// Motivation: `download_repo` formats owner/name/branch straight into
    /// `https://github.com/{owner}/{name}/archive/refs/heads/{branch}.zip`, and URL parsing collapses dot
    /// segments - with a branch of `../../../releases/download/v1/evil` the target becomes that
    /// repository's **release asset**, i.e. arbitrary attacker-uploaded bytes. Once the archive content
    /// is controllable, the extraction path check is the only line of defence, so this layer must hold.
    pub(crate) fn validate_repo_ref(owner: &str, name: &str, branch: &str) -> Result<()> {
        if !Self::is_valid_github_owner(owner) || !Self::is_valid_github_repo_name(name) {
            return Err(anyhow!(format_skill_error(
                "INVALID_REPO_REF",
                &[("owner", owner), ("name", name)],
                Some("checkRepoUrl"),
            )));
        }
        if !Self::is_valid_git_branch(branch) {
            return Err(anyhow!(format_skill_error(
                "INVALID_REPO_REF",
                &[("owner", owner), ("name", name), ("branch", branch)],
                Some("checkRepoUrl"),
            )));
        }
        Ok(())
    }

    /// Exit assertion: after the URL is built, confirm it really points at the expected github.com path.
    ///
    /// Defense in depth - even if the character validation above later misses some variant (percent
    /// encoding, new separator semantics and so on), this still blocks a redirected request.
    fn assert_github_archive_url(url: &str, owner: &str, name: &str) -> Result<()> {
        let parsed = url::Url::parse(url).map_err(|e| anyhow!("Invalid archive URL: {e}"))?;
        let expected_prefix = format!("/{owner}/{name}/archive/refs/heads/");
        if parsed.scheme() != "https"
            || parsed.host_str() != Some("github.com")
            || !parsed.path().starts_with(&expected_prefix)
        {
            return Err(anyhow!(format_skill_error(
                "INVALID_REPO_REF",
                &[("owner", owner), ("name", name)],
                Some("checkRepoUrl"),
            )));
        }
        Ok(())
    }

    /// Find a subdirectory in the tree whose name matches and that contains SKILL.md
    ///
    /// Used by the skills.sh install fallback: the API only returns the skillId (e.g. "find-skills"),
    /// while the real files may live in a repository subdirectory (e.g. "skills/find-skills").
    fn find_skill_dir_by_name(root: &Path, target_name: &str) -> Option<PathBuf> {
        fn walk(dir: &Path, target: &str, depth: usize) -> Option<PathBuf> {
            if depth > 3 {
                return None;
            }
            let entries = fs::read_dir(dir).ok()?;
            for entry in entries.flatten() {
                let path = entry.path();
                if !path.is_dir() {
                    continue;
                }
                let name = entry.file_name();
                let name_str = name.to_string_lossy();
                if name_str.starts_with('.') {
                    continue;
                }
                if name_str.eq_ignore_ascii_case(target) && path.join("SKILL.md").exists() {
                    return Some(path);
                }
                if let Some(found) = walk(&path, target, depth + 1) {
                    return Some(found);
                }
            }
            None
        }
        walk(root, target_name, 0)
    }

    /// Re-resolve the directory information of a discoverable skill into the real source directory inside the extracted tree.
    ///
    /// **Core rule: the returned directory always contains `SKILL.md`** (SKILL.md is the anchor). Resolution order:
    /// 1. A direct relative path hit (e.g. `skills/foo`) validated to contain `SKILL.md` - an explicit path wins;
    /// 2. A recursive search by install name for a directory that matches **and** contains `SKILL.md`;
    /// 3. Fallback: the repository root itself contains `SKILL.md`.
    pub(crate) fn resolve_skill_source_dir(root: &Path, raw_directory: &str) -> Option<PathBuf> {
        let source_rel = Self::sanitize_skill_source_path(raw_directory)?;
        let install_name = source_rel
            .file_name()
            .map(|n| n.to_string_lossy().to_string())?;

        // 1. Direct relative path hit (explicit path wins) - SKILL.md must be verified, otherwise an empty
        //    shell directory of the same name (such as the plugin package directory ast-grep/ under the root of ast-grep/agent-skill) would be mistaken for the source.
        let direct = root.join(&source_rel);
        if direct.is_dir() && direct.join("SKILL.md").is_file() {
            return Some(direct);
        }

        // 2. Recursive search by name (find_skill_dir_by_name already verifies SKILL.md)
        if let Some(found) = Self::find_skill_dir_by_name(root, &install_name) {
            log::info!(
                "Skill directory '{}' not found at direct path, using fallback: {}",
                install_name,
                found.display()
            );
            return Some(found);
        }

        // 3. Fallback: the repository root itself is the skill
        if root.join("SKILL.md").is_file() {
            log::info!(
                "Skill directory '{}' not found, but SKILL.md exists at root, using repo root",
                install_name,
            );
            return Some(root.to_path_buf());
        }

        None
    }

    /// Derive the in-repository relative doc path of SKILL.md from the actually resolved source directory (forward slashes).
    /// Both arguments should already be canonicalized (the install flow already checks containment).
    fn doc_path_for_source(repo_root: &Path, source: &Path) -> Option<String> {
        let rel = source.strip_prefix(repo_root).ok()?;
        let mut parts: Vec<String> = rel
            .components()
            .filter_map(|component| match component {
                std::path::Component::Normal(part) => Some(part.to_string_lossy().to_string()),
                _ => None,
            })
            .collect();
        parts.push("SKILL.md".to_string());
        Some(parts.join("/"))
    }

    /// Pick the in-repository doc path used by readme_url: the actually resolved source directory wins,
    /// then the path stored in the legacy readme_url, and only then a join based on directory. The
    /// skills.sh `directory` is only the skillId (last segment), so a naive join loses the path in nested
    /// layouts and 404s the doc link (#6111); the real source directory must therefore come first.
    fn choose_doc_path(
        resolved_source_doc_path: Option<String>,
        readme_url: Option<&str>,
        directory: &str,
    ) -> String {
        if let Some(path) = resolved_source_doc_path {
            return path;
        }
        if let Some(path) = readme_url.and_then(Self::extract_doc_path_from_url) {
            if path.ends_with("/SKILL.md") || path == "SKILL.md" {
                return path;
            }
            return format!("{}/SKILL.md", path.trim_end_matches('/'));
        }
        format!("{}/SKILL.md", directory.trim_end_matches('/'))
    }

    /// Deduplicate the skill list (by full key, so same-named skills from different repositories stay separate)
    fn deduplicate_discoverable_skills(skills: &mut Vec<DiscoverableSkill>) {
        let mut seen = HashMap::new();
        skills.retain(|skill| {
            // Use the full key (owner/repo:directory) as the unique identifier
            // so that same-named skills from different repositories are listed separately
            let unique_key = skill.key.to_lowercase();
            if let std::collections::hash_map::Entry::Vacant(e) = seen.entry(unique_key) {
                e.insert(true);
                true
            } else {
                false
            }
        });
    }

    /// Download a repository
    ///
    /// This is the **single convergence point** where repository coordinates enter a URL - the four paths
    /// `fetch_repo_skills`, `install`, `check_updates` and `update_skill` all go through it, while `skill_repos` / `skills`
    /// tables are both replaced wholesale by sync-imported remote snapshots that the insert-time validation cannot police. The main defence therefore lives here.
    pub(crate) async fn download_repo(
        &self,
        repo: &SkillRepo,
    ) -> Result<(tempfile::TempDir, String, SkillRepoDelivery)> {
        Self::validate_repo_ref(&repo.owner, &repo.name, &repo.branch)?;

        // The guard is held throughout and handed to the caller together with the directory on success (see the note on `extract_local_zip`).
        // It used to keep() immediately, so any failure - download timeout, ARCHIVE_TOO_LARGE, an
        // extraction error - left half an extracted tree on disk forever, fillable by repeated triggers.
        let temp_dir = tempfile::tempdir()?;
        let temp_path = temp_dir.path().to_path_buf();

        let mut branches = Vec::new();
        if !repo.branch.is_empty() && !repo.branch.eq_ignore_ascii_case("HEAD") {
            branches.push(repo.branch.as_str());
        }
        if !branches.contains(&"main") {
            branches.push("main");
        }
        if !branches.contains(&"master") {
            branches.push("master");
        }

        let mut last_error = None;
        for (branch_index, branch) in branches.iter().enumerate() {
            let url = format!(
                "https://github.com/{}/{}/archive/refs/heads/{}.zip",
                repo.owner, repo.name, branch
            );
            Self::assert_github_archive_url(&url, &repo.owner, &repo.name)?;

            match self.download_and_extract(&url, &temp_path).await {
                Ok(_) => {
                    return Ok((temp_dir, (*branch).to_string(), SkillRepoDelivery::Github));
                }
                Err(failure) => {
                    Self::reset_repo_download_dir(&temp_path)?;
                    match failure.class {
                        RepoTransferFailureClass::NotFound => {
                            // 404 is only used by the existing branch candidate probing and never triggers the mirror.
                            last_error = Some(failure.error);
                            continue;
                        }
                        RepoTransferFailureClass::Terminal => {
                            // Permission, proxy auth, other 4xx, and archive content/budget problems all fail as is.
                            return Err(failure.error);
                        }
                        RepoTransferFailureClass::RetryableTransport => {
                            // One fallback "stage" may keep probing the remaining main/master candidates,
                            // but never switches to a third domain nor retries GitHub.
                            log::warn!(
                                "GitHub Skill source temporarily unavailable for {}/{}; trying the trusted jsDelivr transport once",
                                repo.owner,
                                repo.name
                            );
                            return match self
                                .download_repo_from_jsdelivr(
                                    repo,
                                    &branches[branch_index..],
                                    &temp_path,
                                )
                                .await
                            {
                                Ok(resolved_branch) => {
                                    Ok((temp_dir, resolved_branch, SkillRepoDelivery::Jsdelivr))
                                }
                                Err(mirror_failure) => {
                                    log::warn!(
                                        "Trusted Skill transport fallback failed for {}/{}: {}",
                                        repo.owner,
                                        repo.name,
                                        mirror_failure.error
                                    );
                                    Err(mirror_failure.error)
                                }
                            };
                        }
                    }
                }
            }
        }

        Err(last_error.unwrap_or_else(|| anyhow::anyhow!("All branch downloads failed")))
    }

    fn reset_repo_download_dir(dest: &Path) -> Result<()> {
        if dest.exists() {
            fs::remove_dir_all(dest)?;
        }
        fs::create_dir_all(dest)?;
        Ok(())
    }

    /// Download and extract a ZIP
    async fn download_and_extract(
        &self,
        url: &str,
        dest: &Path,
    ) -> std::result::Result<(), RepoTransferFailure> {
        let client = crate::proxy::http_client::get();
        let response = client.get(url).send().await.map_err(|_| {
            RepoTransferFailure::retryable(anyhow!(format_skill_error(
                "DOWNLOAD_FAILED",
                &[("status", "transport")],
                Some("checkNetwork"),
            )))
        })?;
        if !response.status().is_success() {
            return Err(RepoTransferFailure::from_status(response.status().as_u16()));
        }

        // Read in chunks and cap the compressed size: `response.bytes()` would pull the whole
        // attacker-controlled archive into memory before ZipArchive and the extraction budget get a say -
        // by then the heap is gone. Content-Length cannot be trusted (it can lie or be missing), so the
        // limit is enforced on the bytes actually received.
        let mut response = response;
        let mut body: Vec<u8> = Vec::new();
        while let Some(chunk) = response.chunk().await.map_err(|_| {
            RepoTransferFailure::retryable(anyhow!(format_skill_error(
                "DOWNLOAD_FAILED",
                &[("status", "response_stream")],
                Some("checkNetwork"),
            )))
        })? {
            if body.len().saturating_add(chunk.len()) as u64 > MAX_ARCHIVE_DOWNLOAD_BYTES {
                let limit_mb = (MAX_ARCHIVE_DOWNLOAD_BYTES / 1024 / 1024).to_string();
                return Err(RepoTransferFailure::terminal(anyhow!(format_skill_error(
                    "ARCHIVE_TOO_LARGE",
                    &[("limit_mb", &limit_mb)],
                    Some("checkZipContent"),
                ))));
            }
            body.extend_from_slice(&chunk);
        }

        let cursor = std::io::Cursor::new(body);
        let archive = zip::ZipArchive::new(cursor).map_err(RepoTransferFailure::terminal)?;
        Self::extract_repo_archive(archive, dest).map_err(RepoTransferFailure::terminal)
    }

    async fn download_repo_from_jsdelivr(
        &self,
        repo: &SkillRepo,
        branches: &[&str],
        dest: &Path,
    ) -> std::result::Result<String, RepoTransferFailure> {
        let mut last_error = None;
        for branch in branches {
            match self.download_jsdelivr_repo_branch(repo, branch, dest).await {
                Ok(()) => return Ok((*branch).to_string()),
                Err(failure) if failure.class == RepoTransferFailureClass::NotFound => {
                    Self::reset_repo_download_dir(dest).map_err(RepoTransferFailure::terminal)?;
                    last_error = Some(failure.error);
                }
                Err(failure) => return Err(failure),
            }
        }

        Err(RepoTransferFailure {
            error: last_error.unwrap_or_else(|| {
                anyhow!(format_skill_error(
                    "DOWNLOAD_FAILED",
                    &[("status", "mirror_branch_not_found")],
                    Some("checkNetwork"),
                ))
            }),
            class: RepoTransferFailureClass::NotFound,
        })
    }

    async fn download_jsdelivr_repo_branch(
        &self,
        repo: &SkillRepo,
        branch: &str,
        dest: &Path,
    ) -> std::result::Result<(), RepoTransferFailure> {
        let metadata_url = Self::build_jsdelivr_manifest_url(&repo.owner, &repo.name, branch)
            .map_err(RepoTransferFailure::terminal)?;
        let client = crate::proxy::http_client::get();
        let response = client.get(metadata_url).send().await.map_err(|_| {
            RepoTransferFailure::retryable(anyhow!(format_skill_error(
                "DOWNLOAD_FAILED",
                &[("status", "mirror_transport")],
                Some("checkNetwork"),
            )))
        })?;
        Self::assert_jsdelivr_response_url(response.url(), "cdn.jsdelivr.net")
            .map_err(RepoTransferFailure::terminal)?;
        if !response.status().is_success() {
            return Err(RepoTransferFailure::from_status(response.status().as_u16()));
        }

        let mut response = response;
        let mut metadata = Vec::new();
        while let Some(chunk) = response.chunk().await.map_err(|_| {
            RepoTransferFailure::retryable(anyhow!(format_skill_error(
                "DOWNLOAD_FAILED",
                &[("status", "mirror_metadata_stream")],
                Some("checkNetwork"),
            )))
        })? {
            if metadata.len().saturating_add(chunk.len()) as u64 > MAX_SKILL_MIRROR_METADATA_BYTES {
                return Err(RepoTransferFailure::terminal(anyhow!(format_skill_error(
                    "ARCHIVE_TOO_LARGE",
                    &[("limit_mb", "16")],
                    Some("checkZipContent"),
                ))));
            }
            metadata.extend_from_slice(&chunk);
        }

        let flat: JsdelivrFlatResponse = serde_json::from_slice(&metadata).map_err(|_| {
            RepoTransferFailure::terminal(anyhow!(format_skill_error(
                "INVALID_MIRROR_MANIFEST",
                &[],
                Some("checkRepoUrl"),
            )))
        })?;
        let files = Self::validate_jsdelivr_manifest(repo, branch, flat)
            .map_err(RepoTransferFailure::terminal)?;
        let client_for_files = client.clone();
        let mut downloaded: Vec<(PathBuf, Vec<u8>)> =
            futures::stream::iter(files.into_iter().map(move |file| {
                let client = client_for_files.clone();
                async move { Self::download_jsdelivr_file(&client, file).await }
            }))
            .buffer_unordered(MAX_SKILL_MIRROR_CONCURRENCY)
            .map_err(RepoTransferFailure::terminal)
            .try_collect()
            .await?;

        // Materialization only starts once every entry passed the length + SHA-256 check. Sorting makes
        // the failure position and directory billing reproducible; the temp directory guard cleans up any tree that failed mid-write.
        downloaded.sort_by(|left, right| left.0.cmp(&right.0));
        let mut materialized_bytes = 0u64;
        for (relative_path, bytes) in downloaded {
            let output = dest.join(relative_path);
            if let Some(parent) = output.parent() {
                Self::create_dir_all_within_budget(parent, &mut materialized_bytes)
                    .map_err(RepoTransferFailure::terminal)?;
            }
            Self::charge_archive_budget(&mut materialized_bytes, bytes.len() as u64)
                .map_err(RepoTransferFailure::terminal)?;
            fs::write(output, bytes).map_err(RepoTransferFailure::terminal)?;
        }

        Ok(())
    }

    fn build_jsdelivr_manifest_url(owner: &str, name: &str, branch: &str) -> Result<url::Url> {
        Self::validate_repo_ref(owner, name, branch)?;
        // `+private-json` is the per-snapshot manifest consumed by jsDelivr's
        // own public Data API. Reading it from the same CDN namespace as files
        // avoids the Data API's much longer cache for moving branch aliases.
        let mut url = url::Url::parse("https://cdn.jsdelivr.net/gh/")?;
        {
            let mut segments = url
                .path_segments_mut()
                .map_err(|_| anyhow!("jsDelivr metadata URL cannot be a base URL"))?;
            segments.pop_if_empty();
            segments.push(owner);
            segments.push(&format!("{name}@{branch}"));
            segments.push("+private-json");
        }
        Self::assert_jsdelivr_response_url(&url, "cdn.jsdelivr.net")?;
        Ok(url)
    }

    fn build_jsdelivr_file_url(
        owner: &str,
        name: &str,
        branch: &str,
        relative_path: &str,
    ) -> Result<url::Url> {
        Self::validate_repo_ref(owner, name, branch)?;
        let mut url = url::Url::parse("https://cdn.jsdelivr.net/gh/")?;
        {
            let mut segments = url
                .path_segments_mut()
                .map_err(|_| anyhow!("jsDelivr file URL cannot be a base URL"))?;
            segments.pop_if_empty();
            segments.push(owner);
            segments.push(&format!("{name}@{branch}"));
            for component in relative_path.split('/') {
                segments.push(component);
            }
        }
        Self::assert_jsdelivr_response_url(&url, "cdn.jsdelivr.net")?;
        Ok(url)
    }

    fn assert_jsdelivr_response_url(url: &url::Url, expected_host: &str) -> Result<()> {
        if url.scheme() != "https"
            || url.host_str() != Some(expected_host)
            || !url.username().is_empty()
            || url.password().is_some()
            || url.port().is_some()
            || url.query().is_some()
            || url.fragment().is_some()
        {
            return Err(anyhow!(format_skill_error(
                "INVALID_REPO_REF",
                &[("source", "trusted_mirror")],
                Some("checkRepoUrl"),
            )));
        }
        Ok(())
    }

    fn validate_jsdelivr_manifest(
        repo: &SkillRepo,
        branch: &str,
        flat: JsdelivrFlatResponse,
    ) -> Result<Vec<ValidatedMirrorFile>> {
        if flat.version.as_deref() != Some(branch) {
            return Err(anyhow!(format_skill_error(
                "INVALID_MIRROR_MANIFEST",
                &[("reason", "version_mismatch")],
                Some("checkRepoUrl"),
            )));
        }
        if flat.files.is_empty() {
            return Err(anyhow!(format_skill_error(
                "EMPTY_ARCHIVE",
                &[],
                Some("checkRepoUrl"),
            )));
        }
        if flat.files.len() > MAX_ARCHIVE_ENTRIES {
            return Err(anyhow!(format_skill_error(
                "ARCHIVE_TOO_MANY_ENTRIES",
                &[
                    ("count", &flat.files.len().to_string()),
                    ("limit", &MAX_ARCHIVE_ENTRIES.to_string())
                ],
                Some("checkZipContent"),
            )));
        }

        let mut declared_total = 0u64;
        let mut seen = HashSet::new();
        let mut validated = Vec::with_capacity(flat.files.len());
        for file in flat.files {
            let normalized = Self::normalize_jsdelivr_file_path(&file.name)?;
            if !seen.insert(normalized.clone()) {
                return Err(anyhow!(format_skill_error(
                    "INVALID_MIRROR_MANIFEST",
                    &[("reason", "duplicate_path")],
                    Some("checkZipContent"),
                )));
            }
            declared_total = declared_total.checked_add(file.size).ok_or_else(|| {
                anyhow!(format_skill_error(
                    "ARCHIVE_TOO_LARGE",
                    &[("limit_mb", "128")],
                    Some("checkZipContent"),
                ))
            })?;
            if declared_total > MAX_SKILL_MIRROR_TOTAL_BYTES {
                return Err(anyhow!(format_skill_error(
                    "ARCHIVE_TOO_LARGE",
                    &[("limit_mb", "128")],
                    Some("checkZipContent"),
                )));
            }
            let expected_hash = BASE64_STANDARD.decode(file.hash.as_bytes()).map_err(|_| {
                anyhow!(format_skill_error(
                    "INVALID_MIRROR_MANIFEST",
                    &[("reason", "invalid_hash")],
                    Some("checkZipContent"),
                ))
            })?;
            if expected_hash.len() != 32 {
                return Err(anyhow!(format_skill_error(
                    "INVALID_MIRROR_MANIFEST",
                    &[("reason", "invalid_hash")],
                    Some("checkZipContent"),
                )));
            }
            let relative = normalized.to_string_lossy().replace('\\', "/");
            let url = Self::build_jsdelivr_file_url(&repo.owner, &repo.name, branch, &relative)?;
            validated.push(ValidatedMirrorFile {
                path: normalized,
                url,
                expected_branch: branch.to_string(),
                expected_hash,
                expected_size: file.size,
            });
        }
        Ok(validated)
    }

    fn normalize_jsdelivr_file_path(raw: &str) -> Result<PathBuf> {
        if raw.len() > MAX_SKILL_MIRROR_PATH_BYTES
            || !raw.starts_with('/')
            || raw.starts_with("//")
            || raw.contains('\\')
            || raw.chars().any(char::is_control)
        {
            return Err(anyhow!(format_skill_error(
                "INVALID_MIRROR_MANIFEST",
                &[("reason", "unsafe_path")],
                Some("checkZipContent"),
            )));
        }
        let relative = &raw[1..];
        let parts: Vec<&str> = relative.split('/').collect();
        if parts.is_empty()
            || parts.len() > MAX_SKILL_MIRROR_PATH_COMPONENTS
            || parts
                .iter()
                .any(|part| !Self::is_portable_mirror_path_component(part))
        {
            return Err(anyhow!(format_skill_error(
                "INVALID_MIRROR_MANIFEST",
                &[("reason", "unsafe_path")],
                Some("checkZipContent"),
            )));
        }
        let mut normalized = PathBuf::new();
        for part in parts {
            normalized.push(part);
        }
        Ok(normalized)
    }

    fn is_portable_mirror_path_component(component: &&str) -> bool {
        let component = *component;
        if component.is_empty()
            || component == "."
            || component == ".."
            || component.len() > 255
            || component.ends_with('.')
            || component.ends_with(' ')
            || component
                .chars()
                .any(|character| matches!(character, '<' | '>' | ':' | '"' | '|' | '?' | '*'))
        {
            return false;
        }
        let stem = component.split('.').next().unwrap_or(component);
        let upper = stem.to_ascii_uppercase();
        !matches!(upper.as_str(), "CON" | "PRN" | "AUX" | "NUL")
            && !(upper.len() == 4
                && (upper.starts_with("COM") || upper.starts_with("LPT"))
                && upper.as_bytes()[3].is_ascii_digit()
                && upper.as_bytes()[3] != b'0')
    }

    async fn download_jsdelivr_file(
        client: &reqwest::Client,
        file: ValidatedMirrorFile,
    ) -> Result<(PathBuf, Vec<u8>)> {
        let response = client.get(file.url.clone()).send().await.map_err(|_| {
            anyhow!(format_skill_error(
                "DOWNLOAD_FAILED",
                &[("status", "mirror_file_transport")],
                Some("checkNetwork"),
            ))
        })?;
        Self::assert_jsdelivr_response_url(response.url(), "cdn.jsdelivr.net")?;
        if !response.status().is_success() {
            return Err(RepoTransferFailure::from_status(response.status().as_u16()).error);
        }
        Self::assert_jsdelivr_branch_headers(response.headers(), &file.expected_branch)?;

        let mut response = response;
        let mut body = Vec::with_capacity(
            usize::try_from(file.expected_size)
                .unwrap_or(0)
                .min(1024 * 1024),
        );
        while let Some(chunk) = response.chunk().await.map_err(|_| {
            anyhow!(format_skill_error(
                "DOWNLOAD_FAILED",
                &[("status", "mirror_file_stream")],
                Some("checkNetwork"),
            ))
        })? {
            if body.len().saturating_add(chunk.len()) as u64 > file.expected_size {
                return Err(anyhow!(format_skill_error(
                    "MIRROR_INTEGRITY_FAILED",
                    &[("reason", "size_mismatch")],
                    Some("checkRepoUrl"),
                )));
            }
            body.extend_from_slice(&chunk);
        }
        Self::verify_jsdelivr_file_bytes(file, body)
    }

    fn verify_jsdelivr_file_bytes(
        file: ValidatedMirrorFile,
        body: Vec<u8>,
    ) -> Result<(PathBuf, Vec<u8>)> {
        if body.len() as u64 != file.expected_size {
            return Err(anyhow!(format_skill_error(
                "MIRROR_INTEGRITY_FAILED",
                &[("reason", "size_mismatch")],
                Some("checkRepoUrl"),
            )));
        }
        let actual_hash = Sha256::digest(&body);
        if actual_hash.as_slice() != file.expected_hash.as_slice() {
            return Err(anyhow!(format_skill_error(
                "MIRROR_INTEGRITY_FAILED",
                &[("reason", "hash_mismatch")],
                Some("checkRepoUrl"),
            )));
        }
        Ok((file.path, body))
    }

    fn assert_jsdelivr_branch_headers(
        headers: &reqwest::header::HeaderMap,
        expected_branch: &str,
    ) -> Result<()> {
        let version = headers
            .get("x-jsd-version")
            .and_then(|value| value.to_str().ok());
        let version_type = headers
            .get("x-jsd-version-type")
            .and_then(|value| value.to_str().ok());
        if version != Some(expected_branch) || version_type != Some("branch") {
            return Err(anyhow!(format_skill_error(
                "MIRROR_INTEGRITY_FAILED",
                &[("reason", "branch_identity_mismatch")],
                Some("checkRepoUrl"),
            )));
        }
        Ok(())
    }

    /// Write a single archive entry within the budget, aborting once the total is exceeded.
    ///
    /// Accumulated chunk by chunk instead of reading the size declared in the archive header: that value is written by the archive author and a zip bomb lies.
    fn copy_entry_within_budget<R: std::io::Read, W: std::io::Write>(
        reader: &mut R,
        writer: &mut W,
        total_bytes: &mut u64,
    ) -> Result<()> {
        let mut buffer = [0u8; 16 * 1024];
        loop {
            let read = reader.read(&mut buffer)?;
            if read == 0 {
                return Ok(());
            }
            Self::charge_archive_budget(total_bytes, read as u64)?;
            writer.write_all(&buffer[..read])?;
        }
    }

    /// Read the target path declared by a symlink entry.
    ///
    /// This branch used to be the only extraction without a budget: `read_to_string` pulls the whole
    /// decompressed stream into memory, while the `make_reader` of zip 2.4.2 (read.rs:437-449) only adds
    /// a CRC check and does **not** truncate at the declared uncompressed_size. An entry flagged as a
    /// symlink whose deflate stream expands to several GB is therefore a memory bomb whose budget reading stays 0.
    ///
    /// Anything too long or non-UTF-8 returns `None` so the caller skips it: a legitimate symlink target
    /// is a path, and neither shape can be real data.
    fn read_symlink_target<R: std::io::Read>(
        reader: &mut R,
        total_bytes: &mut u64,
    ) -> Result<Option<String>> {
        let mut raw = Vec::new();
        // Read one extra byte to tell "exactly at the cap" from "truncated"
        let mut limited = std::io::Read::take(reader, MAX_SYMLINK_TARGET_BYTES + 1);
        std::io::Read::read_to_end(&mut limited, &mut raw)?;
        if raw.len() as u64 > MAX_SYMLINK_TARGET_BYTES {
            return Ok(None);
        }
        Self::charge_archive_budget(total_bytes, raw.len() as u64)?;
        Ok(String::from_utf8(raw)
            .ok()
            .map(|target| target.trim().to_string()))
    }

    /// Create directories and bill for the **number of levels actually created**.
    ///
    /// `create_dir_all` creates every missing parent at once, so an entry named `a/a/.../a/f.txt` can
    /// implicitly create hundreds of levels. Billing directories only on the symlink materialization path
    /// is not enough: on the regular extraction path fewer than 10_000 entries can still create millions
    /// of directories while writing almost no content bytes.
    fn create_dir_all_within_budget(path: &Path, total_bytes: &mut u64) -> Result<()> {
        let missing = path.ancestors().take_while(|p| !p.exists()).count() as u64;
        if missing > 0 {
            Self::charge_archive_budget(total_bytes, missing * DIRECTORY_BUDGET_COST)?;
        }
        fs::create_dir_all(path)?;
        Ok(())
    }

    /// The single place where the archive budget is charged.
    ///
    /// It is factored out because "writing file content" is not the only resource an archive can consume:
    /// directories materialized from symlinks write no bytes at all, yet each takes an inode and a
    /// directory block, and the second symlink resolution pass can grow the directory count exponentially
    /// with depth. Billing content bytes only would let an archive of empty directories keep the budget at 0.
    fn charge_archive_budget(total_bytes: &mut u64, amount: u64) -> Result<()> {
        if total_bytes.saturating_add(amount) > MAX_ARCHIVE_TOTAL_BYTES {
            let limit_mb = (MAX_ARCHIVE_TOTAL_BYTES / 1024 / 1024).to_string();
            return Err(anyhow::anyhow!(format_skill_error(
                "ARCHIVE_TOO_LARGE",
                &[("limit_mb", &limit_mb)],
                Some("checkZipContent"),
            )));
        }
        *total_bytes += amount;
        Ok(())
    }

    /// Extract a GitHub repository archive into `dest` (stripping the single root directory the archive carries).
    ///
    /// Kept separate from `download_and_extract` so the zip-slip protection can be unit tested offline.
    fn extract_repo_archive<R: std::io::Read + std::io::Seek>(
        mut archive: zip::ZipArchive<R>,
        dest: &Path,
    ) -> Result<()> {
        let root_name = if !archive.is_empty() {
            let first_file = archive.by_index(0)?;
            let name = first_file.name();
            name.split('/').next().unwrap_or("").to_string()
        } else {
            return Err(anyhow::anyhow!(format_skill_error(
                "EMPTY_ARCHIVE",
                &[],
                Some("checkRepoUrl"),
            )));
        };

        // Archive bytes are fully third-party controlled (repositories can be added via deeplink), so the
        // extraction must be capped, otherwise a few-MB zip bomb fills the disk. webdav_sync/archive.rs
        // has had the same dual cap for a long time; this download path never did.
        if archive.len() > MAX_ARCHIVE_ENTRIES {
            let count = archive.len().to_string();
            let limit = MAX_ARCHIVE_ENTRIES.to_string();
            return Err(anyhow::anyhow!(format_skill_error(
                "ARCHIVE_TOO_MANY_ENTRIES",
                &[("count", &count), ("limit", &limit)],
                Some("checkZipContent"),
            )));
        }
        let mut total_bytes: u64 = 0;

        // First pass: extract regular files and directories, collecting the symlink entries
        let mut symlinks: Vec<(PathBuf, String)> = Vec::new();

        for i in 0..archive.len() {
            let mut file = archive.by_index(i)?;
            // First gate: enclosed_name() rejects absolute paths, drive prefixes and entries with a
            // negative net depth (i.e. escaping the archive root). Skill repositories can be added by
            // deeplink, so archive content is third-party controllable input.
            let Some(safe_path) = file.enclosed_name() else {
                log::warn!("Skipping an unsafe archive entry: {}", file.name());
                continue;
            };

            // GitHub archives always carry one `<repo>-<branch>/` root directory that must be stripped before writing.
            let Ok(relative_path) = safe_path.strip_prefix(&root_name) else {
                continue;
            };

            // Second gate: the guarantee of enclosed_name() is relative to the **archive root** and it
            // does not normalize paths - `..` stays in the returned value. Stripping root_name above
            // spends one level of depth budget, so an entry like `repo-main/../evil` can still land
            // outside dest (one level on Unix; on Windows root_name may contain backslashes and count as
            // several segments, amplifying the escape). The **relative path actually used** must therefore be re-validated before the join.
            if relative_path
                .components()
                .any(|c| matches!(c, Component::ParentDir))
            {
                log::warn!("Skipping an out-of-bounds archive entry: {}", file.name());
                continue;
            }

            if relative_path.as_os_str().is_empty() {
                continue;
            }

            let outpath = dest.join(relative_path);

            if file.is_symlink() {
                let Some(target) = Self::read_symlink_target(&mut file, &mut total_bytes)? else {
                    log::warn!(
                        "Skipping a symlink entry with an invalid target: {}",
                        file.name()
                    );
                    continue;
                };
                symlinks.push((outpath, target));
            } else if file.is_dir() {
                Self::create_dir_all_within_budget(&outpath, &mut total_bytes)?;
            } else {
                if let Some(parent) = outpath.parent() {
                    Self::create_dir_all_within_budget(parent, &mut total_bytes)?;
                }
                let mut outfile = fs::File::create(&outpath)?;
                // Accumulate the bytes actually written rather than trusting the size declared in the
                // archive header - a zip bomb can declare anything.
                Self::copy_entry_within_budget(&mut file, &mut outfile, &mut total_bytes)?;
            }
        }

        // Second pass: resolve the symlinks, copying the target content to the symlink location
        Self::resolve_symlinks_in_dir(dest, &symlinks, &mut total_bytes)?;

        Ok(())
    }

    /// Same semantics as `copy_dir_recursive`, but the written bytes count towards the archive budget.
    /// Only used to materialize symlinks during extraction - regular directory copies (install, backup,
    /// migration) must not be constrained by the archive budget, so the two functions are deliberately separate.
    fn copy_dir_within_budget(src: &Path, dest: &Path, total_bytes: &mut u64) -> Result<()> {
        Self::create_dir_all_within_budget(dest, total_bytes)?;

        for entry in fs::read_dir(src)? {
            let entry = entry?;
            let path = entry.path();
            let dest_path = dest.join(entry.file_name());

            if path.is_dir() {
                Self::copy_dir_within_budget(&path, &dest_path, total_bytes)?;
            } else {
                Self::copy_file_within_budget(&path, &dest_path, total_bytes)?;
            }
        }

        Ok(())
    }

    /// Copy a single file and charge it to the archive budget, reusing `copy_entry_within_budget` so the
    /// cap and the error message are defined in one place only.
    fn copy_file_within_budget(src: &Path, dest: &Path, total_bytes: &mut u64) -> Result<()> {
        let mut reader = fs::File::open(src)?;
        let mut writer = fs::File::create(dest)?;
        Self::copy_entry_within_budget(&mut reader, &mut writer, total_bytes)
    }

    /// Same shape as `copy_dir_recursive`, but the entry type comes from `entry.file_type()` -
    /// `Path::is_dir()` and `fs::copy` both follow symbolic links.
    ///
    /// Following is harmless when the source is the SSOT this product writes itself; when copying a
    /// third-party directory to another tool, following would wire that tool to a location this product
    /// neither owns nor controls. A symlink entry is therefore an error here instead of being silently materialized into a real file.
    fn copy_dir_recursive_without_links(src: &Path, dest: &Path) -> Result<()> {
        fs::create_dir_all(dest)?;

        for entry in fs::read_dir(src)? {
            let entry = entry?;
            let file_type = entry.file_type()?;
            let path = entry.path();
            let dest_path = dest.join(entry.file_name());

            if file_type.is_symlink() {
                return Err(anyhow!(
                    "The skill source directory contains a symbolic link, refusing to copy: {}",
                    path.display()
                ));
            }

            if file_type.is_dir() {
                Self::copy_dir_recursive_without_links(&path, &dest_path)?;
            } else {
                fs::copy(&path, &dest_path)?;
            }
        }

        Ok(())
    }

    /// Recursively copy a directory
    fn copy_dir_recursive(src: &Path, dest: &Path) -> Result<()> {
        fs::create_dir_all(dest)?;

        for entry in fs::read_dir(src)? {
            let entry = entry?;
            let path = entry.path();
            let dest_path = dest.join(entry.file_name());

            if path.is_dir() {
                Self::copy_dir_recursive(&path, &dest_path)?;
            } else {
                fs::copy(&path, &dest_path)?;
            }
        }

        Ok(())
    }

    fn resolve_uninstall_backup_source_excluding(
        skill: &InstalledSkill,
        excluded_path: Option<&Path>,
    ) -> Result<Option<PathBuf>> {
        // The return value is copied wholesale into the product AppData/skill-backups/ and listed in the
        // UI by get_skill_backups - a dirty directory here is an arbitrary file read plus an exfiltration channel.
        let directory = Self::require_valid_directory(&skill.directory)?;

        let ssot_path = Self::get_ssot_dir()?.join(&directory);
        if ssot_path.is_dir()
            && !excluded_path.is_some_and(|path| Self::paths_overlap(&ssot_path, path))
        {
            return Ok(Some(ssot_path));
        }

        for app in AppType::all() {
            let app_dir = match Self::get_app_skills_dir(&app) {
                Ok(dir) => dir,
                Err(_) => continue,
            };
            let candidate = app_dir.join(&directory);
            if candidate.is_dir()
                && !excluded_path.is_some_and(|path| Self::paths_overlap(&candidate, path))
            {
                return Ok(Some(candidate));
            }
        }

        Ok(None)
    }

    fn sanitize_backup_segment(segment: &str) -> String {
        let sanitized = segment
            .chars()
            .map(|c| match c {
                'a'..='z' | 'A'..='Z' | '0'..='9' | '-' | '_' | '.' => c,
                _ => '-',
            })
            .collect::<String>()
            .trim_matches('-')
            .to_string();

        if sanitized.is_empty() {
            "skill".to_string()
        } else {
            sanitized
        }
    }

    fn cleanup_old_skill_backups(dir: &Path) -> Result<()> {
        let mut entries = fs::read_dir(dir)?
            .filter_map(|entry| entry.ok())
            .filter_map(|entry| {
                let metadata = entry.metadata().ok()?;
                if !metadata.is_dir() {
                    return None;
                }
                Some((entry.path(), metadata.modified().ok()))
            })
            .collect::<Vec<_>>();

        if entries.len() <= SKILL_BACKUP_RETAIN_COUNT {
            return Ok(());
        }

        entries.sort_by_key(|(_, modified)| *modified);
        let remove_count = entries.len().saturating_sub(SKILL_BACKUP_RETAIN_COUNT);

        for (path, _) in entries.into_iter().take(remove_count) {
            fs::remove_dir_all(&path)?;
        }

        Ok(())
    }

    fn backup_path_for_id(backup_id: &str) -> Result<PathBuf> {
        if backup_id.contains("..")
            || backup_id.contains('/')
            || backup_id.contains('\\')
            || backup_id.trim().is_empty()
        {
            return Err(anyhow!("Invalid backup id: {backup_id}"));
        }

        Ok(Self::get_backup_dir()?.join(backup_id))
    }

    fn read_backup_metadata(backup_path: &Path) -> Result<SkillBackupMetadata> {
        let metadata_path = backup_path.join("meta.json");
        let content = fs::read_to_string(&metadata_path)
            .with_context(|| format!("failed to read {}", metadata_path.display()))?;
        serde_json::from_str(&content)
            .with_context(|| format!("failed to parse {}", metadata_path.display()))
    }

    fn create_uninstall_backup(skill: &InstalledSkill) -> Result<Option<PathBuf>> {
        Self::create_uninstall_backup_excluding(skill, None)
    }

    fn create_uninstall_backup_excluding(
        skill: &InstalledSkill,
        excluded_path: Option<&Path>,
    ) -> Result<Option<PathBuf>> {
        let Some(source_path) =
            Self::resolve_uninstall_backup_source_excluding(skill, excluded_path)?
        else {
            log::warn!(
                "No backup-able directory found before uninstalling skill {}, skipping the backup",
                skill.directory
            );
            return Ok(None);
        };

        let backup_root = Self::get_backup_dir()?;
        let timestamp = Utc::now().format("%Y%m%d_%H%M%S");
        let slug = Self::sanitize_backup_segment(&skill.directory);
        let mut backup_path = backup_root.join(format!("{timestamp}_{slug}"));
        let mut counter = 1;
        while backup_path.exists() {
            backup_path = backup_root.join(format!("{timestamp}_{slug}_{counter}"));
            counter += 1;
        }

        let write_backup = || -> Result<()> {
            let skill_backup_dir = backup_path.join("skill");
            Self::copy_dir_recursive(&source_path, &skill_backup_dir)?;

            let metadata = SkillBackupMetadata {
                skill: skill.clone(),
                backup_created_at: Utc::now().timestamp(),
                source_path: source_path.to_string_lossy().to_string(),
            };
            let metadata_path = backup_path.join("meta.json");
            let metadata_json = serde_json::to_string_pretty(&metadata)
                .context("failed to serialize skill backup metadata")?;
            fs::write(&metadata_path, metadata_json)
                .with_context(|| format!("failed to write {}", metadata_path.display()))?;
            Ok(())
        };

        if let Err(err) = write_backup() {
            let _ = fs::remove_dir_all(&backup_path);
            return Err(err);
        }

        if let Err(err) = Self::cleanup_old_skill_backups(&backup_root) {
            log::warn!("Failed to clean up old skill backups: {err:#}");
        }

        log::info!(
            "Skill {} was backed up to {} before the uninstall",
            skill.name,
            backup_path.display()
        );

        Ok(Some(backup_path))
    }

    /// Resolve symlinks in a ZIP: copy the target content into the symlink location
    ///
    /// GitHub ZIP archives keep the symlink metadata, detectable with `is_symlink()` during extraction.
    /// This method resolves a symlink into the actual file/directory content (instead of creating a real
    /// symlink), keeping it cross-platform compatible and the skill content self-contained.
    fn resolve_symlinks_in_dir(
        base_dir: &Path,
        symlinks: &[(PathBuf, String)],
        total_bytes: &mut u64,
    ) -> Result<()> {
        // Canonicalize base_dir (on macOS /tmp -> /private/tmp, which must stay consistent)
        let canonical_base = base_dir
            .canonicalize()
            .unwrap_or_else(|_| base_dir.to_path_buf());

        for (link_path, target) in symlinks {
            // Take the parent directory of the symlink, then join the relative target path
            let parent = link_path.parent().unwrap_or(base_dir);
            let resolved = parent.join(target);

            // Normalize the path (resolving .. and friends)
            let resolved = match resolved.canonicalize() {
                Ok(p) => p,
                Err(_) => {
                    log::warn!(
                        "Symlink target does not exist, skipping: {} -> {}",
                        link_path.display(),
                        target
                    );
                    continue;
                }
            };

            // Safety check one: make sure the target is inside base_dir (preventing traversal)
            if !resolved.starts_with(&canonical_base) {
                log::warn!(
                    "Symlink target is outside the repository, skipping: {} -> {}",
                    link_path.display(),
                    resolved.display()
                );
                continue;
            }

            // Safety check two: the target must not contain the link itself. The check above guards
            // against "escaping base" but not against "nesting into itself" - `dir/link -> ..` resolves
            // to base itself, which is perfectly legal, and the recursive copy would then copy the
            // archive root into its own subdirectory, seeing the freshly written copy at every level until PATH_MAX fails.
            //
            // The comparison must be done on the **canonical form**: `enclosed_name()` does not normalize
            // paths and only guarantees a non-negative net depth, so link_path may still carry unresolved
            // `..` (`e/../d/self`). Comparing it literally against the canonicalized resolved path would
            // diverge at the first segment (`e` vs `d`), making the check useless. link_path itself is not
            // on disk yet, but its parent must exist - a successful canonicalize of `resolved` implies it.
            let canonical_link = match parent.canonicalize() {
                Ok(canonical_parent) => match link_path.file_name() {
                    Some(name) => canonical_parent.join(name),
                    None => canonical_parent,
                },
                // Fall back to the literal form when even the parent is missing: resolved would most
                // likely have failed to resolve already and continued above; this only keeps the guard from panicking on an unexpected shape.
                Err(_) => match link_path.strip_prefix(base_dir) {
                    Ok(relative) => canonical_base.join(relative),
                    Err(_) => link_path.clone(),
                },
            };
            if canonical_link.starts_with(&resolved) {
                log::warn!(
                    "Symlink target contains the link itself, skipping (it would cause a recursive self copy): {} -> {}",
                    link_path.display(),
                    resolved.display()
                );
                continue;
            }

            // Copy the target content to the symlink location. It must share the byte budget with the
            // extraction loop: materialization takes this separate path, and without billing, "one big
            // file + N symlinks to it" could write N times the bytes while MAX_ARCHIVE_TOTAL_BYTES still looked compliant.
            if resolved.is_dir() {
                Self::copy_dir_within_budget(&resolved, link_path, total_bytes)?;
            } else if resolved.is_file() {
                if let Some(parent) = link_path.parent() {
                    Self::create_dir_all_within_budget(parent, total_bytes)?;
                }
                Self::copy_file_within_budget(&resolved, link_path, total_bytes)?;
            }
        }
        Ok(())
    }

    // ========== Installing from a ZIP file ==========

    /// Install skills from a local ZIP file
    ///
    /// Flow:
    /// 1. Extract the ZIP into a temporary directory
    /// 2. Scan the directories for skills containing SKILL.md
    /// 3. Copy into the SSOT and save to the database
    /// 4. Sync to the current app directories
    pub fn install_from_zip(
        db: &Arc<Database>,
        zip_path: &Path,
        current_app: &AppType,
    ) -> Result<Vec<InstalledSkill>> {
        // Extract into a temporary directory
        let temp_guard = Self::extract_local_zip(zip_path)?;
        let temp_dir = temp_guard.path();

        // Scan every directory containing SKILL.md
        let skill_dirs = Self::scan_skills_in_dir(temp_dir)?;

        if skill_dirs.is_empty() {
            return Err(anyhow!(format_skill_error(
                "NO_SKILLS_IN_ZIP",
                &[],
                Some("checkZipContent"),
            )));
        }

        let _state_guard = skill_state_write_guard();
        let ssot_dir = Self::get_ssot_dir()?;
        let mut installed = Vec::new();
        let existing_skills = db.get_all_installed_skills()?;
        let zip_stem = zip_path
            .file_stem()
            .and_then(|s| s.to_str())
            .map(|s| s.to_string());

        for skill_dir in skill_dirs {
            // Parse the metadata (done early, used to determine the install name)
            let skill_md = skill_dir.join("SKILL.md");
            let meta = if skill_md.exists() {
                Self::parse_skill_metadata_static(&skill_md).ok()
            } else {
                None
            };

            // Use the directory name as the install name
            // When SKILL.md sits at the ZIP root, skill_dir == temp_dir and
            // file_name() returns the temp directory name (e.g. .tmpDZKGpF), so other sources are needed
            let install_name = {
                let dir_name = skill_dir
                    .file_name()
                    .map(|s| s.to_string_lossy().to_string())
                    .unwrap_or_default();

                if skill_dir.as_path() == temp_dir
                    || dir_name.is_empty()
                    || dir_name.starts_with('.')
                {
                    // SKILL.md at the root: prefer the metadata name, otherwise the ZIP file name
                    meta.as_ref()
                        .and_then(|m| m.name.as_deref())
                        .and_then(Self::sanitize_install_name)
                        .or_else(|| zip_stem.as_deref().and_then(Self::sanitize_install_name))
                } else {
                    Self::sanitize_install_name(&dir_name)
                        .or_else(|| {
                            meta.as_ref()
                                .and_then(|m| m.name.as_deref())
                                .and_then(Self::sanitize_install_name)
                        })
                        .or_else(|| zip_stem.as_deref().and_then(Self::sanitize_install_name))
                }
            };
            let install_name = match install_name {
                Some(name) => name,
                None => {
                    return Err(anyhow!(format_skill_error(
                        "INVALID_SKILL_DIRECTORY",
                        &[("zip", &zip_path.display().to_string())],
                        Some("checkZipContent"),
                    )));
                }
            };

            // Check whether a skill with the same directory already exists
            let conflict = existing_skills
                .values()
                .find(|s| s.directory.eq_ignore_ascii_case(&install_name));

            if let Some(existing) = conflict {
                log::warn!(
                    "Skill directory '{}' already exists (from {}), skipping",
                    install_name,
                    existing.id
                );
                continue;
            }

            let (name, description) = match meta {
                Some(m) => (
                    m.name.unwrap_or_else(|| install_name.clone()),
                    m.description,
                ),
                None => (install_name.clone(), None),
            };

            Self::preflight_install_destination(&skill_dir, &install_name, current_app)?;

            // Copy into the SSOT
            let dest = ssot_dir.join(&install_name);
            if dest.exists() {
                let _ = fs::remove_dir_all(&dest);
            }
            Self::copy_dir_recursive(&skill_dir, &dest)?;

            // Compute the content hash
            let content_hash = Self::compute_dir_hash(&dest).ok();

            // Create the InstalledSkill record
            let skill = InstalledSkill {
                id: format!("local:{install_name}"),
                name,
                description,
                directory: install_name.clone(),
                repo_owner: None,
                repo_name: None,
                repo_branch: None,
                readme_url: None,
                apps: SkillApps::only(current_app),
                installed_at: chrono::Utc::now().timestamp(),
                content_hash,
                updated_at: 0,
            };

            Self::persist_and_sync_new_skill(db, &skill, current_app)?;

            log::info!(
                "Skill {} installed from ZIP, enabled for {:?}",
                skill.name,
                current_app
            );
            installed.push(skill);
        }

        Ok(installed)
    }

    /// Extract a local ZIP file into a temporary directory
    ///
    /// Returns a `TempDir` rather than a `PathBuf`: after extraction the caller still has a long chain of
    /// `?` for scanning, copying, writing to the database and syncing, and an early return at any of them
    /// would leave up to 512 MiB of temporary content on disk forever. Handing the guard to the caller
    /// makes cleanup automatic at scope exit instead of relying on every exit remembering `remove_dir_all` (more than one had forgotten).
    fn extract_local_zip(zip_path: &Path) -> Result<tempfile::TempDir> {
        Self::extract_local_zip_in(zip_path, &std::env::temp_dir())
    }

    /// Same as [`Self::extract_local_zip`], but the caller chooses where the temporary directory lands.
    /// Tests use it to pin the extraction root inside a private directory instead of hijacking the
    /// process-wide `TMPDIR` - the latter would suck the temp directories of concurrent tests into the observed directory and randomly break "the directory must be empty" assertions.
    fn extract_local_zip_in(zip_path: &Path, base_dir: &Path) -> Result<tempfile::TempDir> {
        let file = fs::File::open(zip_path)
            .with_context(|| format!("Failed to open ZIP file: {}", zip_path.display()))?;

        let mut archive = zip::ZipArchive::new(file)
            .with_context(|| format!("Failed to read ZIP file: {}", zip_path.display()))?;

        if archive.is_empty() {
            return Err(anyhow!(format_skill_error(
                "EMPTY_ARCHIVE",
                &[],
                Some("checkZipContent"),
            )));
        }

        // Same caps as for remote archives. A local ZIP is a user-chosen file (more trusted), but "the
        // user was tricked into opening a zip bomb" is still a common path, and both extraction paths share one materializer.
        if archive.len() > MAX_ARCHIVE_ENTRIES {
            let count = archive.len().to_string();
            let limit = MAX_ARCHIVE_ENTRIES.to_string();
            return Err(anyhow!(format_skill_error(
                "ARCHIVE_TOO_MANY_ENTRIES",
                &[("count", &count), ("limit", &limit)],
                Some("checkZipContent"),
            )));
        }

        // The guard is held until the extraction fully succeeds: any `?` in between makes it clean up the half-written directory.
        // It used to keep() right here, so exceeding the cap or an extraction error left permanent residue.
        let temp_dir = tempfile::tempdir_in(base_dir)?;
        let temp_path = temp_dir.path().to_path_buf();

        let mut symlinks: Vec<(PathBuf, String)> = Vec::new();
        let mut total_bytes: u64 = 0;

        for i in 0..archive.len() {
            let mut file = archive.by_index(i)?;
            let file_path = match file.enclosed_name() {
                Some(path) => path.to_owned(),
                None => continue,
            };

            // `enclosed_name()` only guarantees a non-negative net depth and does **not** resolve `..`.
            // There is no `strip_prefix` eating depth budget here, so nothing escapes temp_path; but
            // leaving unresolved paths would make the later symlink self-containment check incomparable
            // (`e/../d/self` versus `d` diverge at the first component). Rejecting them at the entry keeps
            // every path in the symlinks table in canonical shape.
            if file_path
                .components()
                .any(|c| matches!(c, Component::ParentDir))
            {
                log::warn!("Skipping an out-of-bounds archive entry: {}", file.name());
                continue;
            }

            let outpath = temp_path.join(&file_path);

            if file.is_symlink() {
                let Some(target) = Self::read_symlink_target(&mut file, &mut total_bytes)? else {
                    log::warn!(
                        "Skipping a symlink entry with an invalid target: {}",
                        file.name()
                    );
                    continue;
                };
                symlinks.push((outpath, target));
            } else if file.is_dir() {
                Self::create_dir_all_within_budget(&outpath, &mut total_bytes)?;
            } else {
                if let Some(parent) = outpath.parent() {
                    Self::create_dir_all_within_budget(parent, &mut total_bytes)?;
                }
                let mut outfile = fs::File::create(&outpath)?;
                Self::copy_entry_within_budget(&mut file, &mut outfile, &mut total_bytes)?;
            }
        }

        // Resolve the symlinks
        Self::resolve_symlinks_in_dir(&temp_path, &symlinks, &mut total_bytes)?;

        Ok(temp_dir)
    }

    /// Recursively scan a directory for skill directories containing SKILL.md
    fn scan_skills_in_dir(dir: &Path) -> Result<Vec<PathBuf>> {
        let mut skill_dirs = Vec::new();
        Self::scan_skills_recursive(dir, &mut skill_dirs)?;
        Ok(skill_dirs)
    }

    /// Recursive scan helper
    fn scan_skills_recursive(current: &Path, results: &mut Vec<PathBuf>) -> Result<()> {
        // Check whether the current directory contains SKILL.md
        let skill_md = current.join("SKILL.md");
        if skill_md.exists() {
            results.push(current.to_path_buf());
            // Once found, do not recurse into subdirectories (one skill directory)
            return Ok(());
        }

        // Recurse into subdirectories
        if let Ok(entries) = fs::read_dir(current) {
            for entry in entries.flatten() {
                let path = entry.path();
                if path.is_dir() {
                    // Skip hidden directories
                    let dir_name = entry.file_name().to_string_lossy().to_string();
                    if dir_name.starts_with('.') {
                        continue;
                    }
                    Self::scan_skills_recursive(&path, results)?;
                }
            }
        }

        Ok(())
    }

    // ========== Repository management (existing logic kept) ==========

    /// List the repositories
    pub fn list_repos(&self, store: &SkillStore) -> Vec<SkillRepo> {
        store.repos.clone()
    }

    /// Add a repository
    pub fn add_repo(&self, store: &mut SkillStore, repo: SkillRepo) -> Result<()> {
        if let Some(pos) = store
            .repos
            .iter()
            .position(|r| r.owner == repo.owner && r.name == repo.name)
        {
            store.repos[pos] = repo;
        } else {
            store.repos.push(repo);
        }

        Ok(())
    }

    /// Delete a repository
    pub fn remove_repo(&self, store: &mut SkillStore, owner: String, name: String) -> Result<()> {
        store
            .repos
            .retain(|r| !(r.owner == owner && r.name == name));

        Ok(())
    }

    // ========== skills.sh search ==========

    /// Search the public skills.sh catalog
    pub async fn search_skills_sh(
        query: &str,
        limit: usize,
        offset: usize,
    ) -> Result<SkillsShSearchResult> {
        let client = crate::proxy::http_client::get();

        let url = url::Url::parse_with_params(
            "https://skills.sh/api/search",
            &[
                ("q", query),
                ("limit", &limit.to_string()),
                ("offset", &offset.to_string()),
            ],
        )?;

        let resp = client
            .get(url)
            .timeout(std::time::Duration::from_secs(10))
            .send()
            .await?
            .error_for_status()?
            .json::<SkillsShApiResponse>()
            .await?;

        let skills = resp
            .skills
            .into_iter()
            .filter_map(|s| {
                let parts: Vec<&str> = s.source.splitn(2, '/').collect();
                if parts.len() != 2 {
                    return None;
                }
                let (owner, repo) = (parts[0].to_string(), parts[1].to_string());
                // Use the same coordinate validation as download_repo instead of an ad-hoc heuristic:
                // the readme_url below is eventually opened with openExternal, the same sink as
                // build_skill_doc_url. The old `contains('.')` both missed cases (`splitn(2, '/')`
                // allows a `/` inside repo, so `owner/a/b` builds a three-segment path) and caused false
                // positives (GitHub repository names legitimately contain dots). Validating owner also
                // keeps the "filter out non-GitHub sources" effect - a dotted owner like `skills.volces.com` is not a valid user name anyway.
                if Self::validate_repo_ref(&owner, &repo, "main").is_err() {
                    return None;
                }
                Some(SkillsShDiscoverableSkill {
                    key: s.id,
                    name: s.name,
                    directory: s.skill_id.clone(),
                    repo_owner: owner.clone(),
                    repo_name: repo.clone(),
                    repo_branch: "main".to_string(),
                    installs: s.installs,
                    readme_url: Some(format!("https://github.com/{}/{}", owner, repo)),
                })
            })
            .collect();

        Ok(SkillsShSearchResult {
            skills,
            total_count: resp.count,
            query: resp.query,
        })
    }
}

// ========== Migration support ==========

/// Build the skill ID, repository fields and readme URL from the lock file info
///
/// Returns (id, repo_owner, repo_name, repo_branch, readme_url)
fn build_repo_info_from_lock(
    lock: &HashMap<String, LockRepoInfo>,
    dir_name: &str,
) -> (
    String,
    Option<String>,
    Option<String>,
    Option<String>,
    Option<String>,
) {
    match lock.get(dir_name) {
        Some(info) => {
            let branch = info.branch.clone();
            let url_branch = branch.clone().unwrap_or_else(|| "HEAD".to_string());
            // Prefer the skillPath from the lock file, otherwise fall back to dir_name/SKILL.md
            let fallback = format!("{dir_name}/SKILL.md");
            let doc_path = info.skill_path.as_deref().unwrap_or(&fallback);
            let url =
                SkillService::build_skill_doc_url(&info.owner, &info.repo, &url_branch, doc_path);
            (
                format!("{}/{}:{dir_name}", info.owner, info.repo),
                Some(info.owner.clone()),
                Some(info.repo.clone()),
                branch,
                url,
            )
        }
        None => (format!("local:{dir_name}"), None, None, None, None),
    }
}

/// Save the repositories discovered in the lock file into skill_repos (deduplicated)
fn save_repos_from_lock(
    db: &Arc<Database>,
    lock: &HashMap<String, LockRepoInfo>,
    directories: impl Iterator<Item = impl AsRef<str>>,
) {
    let existing_repos: HashSet<(String, String)> = db
        .get_skill_repos()
        .unwrap_or_default()
        .into_iter()
        .map(|r| (r.owner, r.name))
        .collect();
    let mut added = HashSet::new();

    for dir_name in directories {
        if let Some(info) = lock.get(dir_name.as_ref()) {
            let key = (info.owner.clone(), info.repo.clone());
            if !existing_repos.contains(&key) && added.insert(key) {
                let skill_repo = SkillRepo {
                    owner: info.owner.clone(),
                    name: info.repo.clone(),
                    // Use the HEAD semantics for an unknown branch; the download later falls back to main/master.
                    branch: info.branch.clone().unwrap_or_else(|| "HEAD".to_string()),
                    enabled: true,
                };
                // The lock file is written by the external agents CLI, so owner/repo/branch are unvalidated
                // and branch is a raw string scraped out of `/tree/`, a fragment or `?ref=`.
                if SkillService::validate_repo_ref(
                    &skill_repo.owner,
                    &skill_repo.name,
                    &skill_repo.branch,
                )
                .is_err()
                {
                    log::warn!(
                        "Skipping a repository with invalid coordinates in the agents lock: {}/{}@{}",
                        skill_repo.owner,
                        skill_repo.name,
                        skill_repo.branch
                    );
                    continue;
                }
                if let Err(e) = db.save_skill_repo(&skill_repo) {
                    log::warn!(
                        "Failed to save skill repository {}/{}: {}",
                        info.owner,
                        info.repo,
                        e
                    );
                } else {
                    log::info!(
                        "Discovered and added a repository from the agents lock file: {}/{} ({})",
                        info.owner,
                        info.repo,
                        skill_repo.branch
                    );
                }
            }
        }
    }
}

/// First-launch migration: scan the app directories and rebuild the database
pub fn migrate_skills_to_ssot(db: &Arc<Database>) -> Result<usize> {
    let _state_guard = skill_state_write_guard();
    let ssot_dir = SkillService::get_ssot_dir()?;
    let agents_lock = parse_agents_lock();
    let snapshot: Vec<LegacySkillMigrationRow> = match db
        .get_setting("skills_ssot_migration_snapshot")?
    {
        Some(value) if !value.trim().is_empty() => match serde_json::from_str(&value) {
            Ok(rows) => rows,
            Err(err) => {
                log::warn!("Failed to parse the skills migration snapshot, falling back to a filesystem scan: {err}");
                Vec::new()
            }
        },
        _ => Vec::new(),
    };

    let has_snapshot = !snapshot.is_empty();
    let mut discovered: HashMap<String, SkillApps> = HashMap::new();

    if has_snapshot {
        for row in &snapshot {
            // The snapshot lives in the settings table, and settings are within the sync scope and can be
            // overwritten by a remote snapshot. Every key of discovered below is joined into a path and
            // written back into the skills table, so dirty values must be filtered before entering discovered.
            if SkillService::require_valid_directory(&row.directory).is_err() {
                log::warn!(
                    "Skipping an invalid directory in the SSOT migration snapshot: {:?}",
                    row.directory
                );
                continue;
            }
            if let Ok(app) = row.app_type.parse::<AppType>() {
                discovered
                    .entry(row.directory.clone())
                    .or_default()
                    .set_enabled_for(&app, true);
            }
        }
    }

    // Scan every app directory
    for app in AppType::all() {
        let app_dir = match SkillService::get_app_skills_dir(&app) {
            Ok(d) => d,
            Err(_) => continue,
        };

        let entries = match fs::read_dir(&app_dir) {
            Ok(e) => e,
            Err(_) => continue,
        };

        for entry in entries.flatten() {
            let path = entry.path();
            if !path.is_dir() {
                continue;
            }

            let dir_name = entry.file_name().to_string_lossy().to_string();
            if dir_name.starts_with('.') {
                continue;
            }
            if !path.join("SKILL.md").exists() {
                continue;
            }
            if has_snapshot && !discovered.contains_key(&dir_name) {
                continue;
            }

            // Copy into the SSOT (when absent)
            let ssot_path = ssot_dir.join(&dir_name);
            if !ssot_path.exists() {
                SkillService::copy_dir_recursive(&path, &ssot_path)?;
            }

            if !has_snapshot {
                discovered
                    .entry(dir_name)
                    .or_default()
                    .set_enabled_for(&app, true);
            }
        }
    }

    // Rebuild the database
    db.clear_skills()?;

    // Save the repositories discovered in the lock file into skill_repos
    save_repos_from_lock(db, &agents_lock, discovered.keys());

    let mut count = 0;
    for (directory, apps) in discovered {
        let ssot_path = ssot_dir.join(&directory);
        let skill_md = ssot_path.join("SKILL.md");

        let (name, description) = SkillService::read_skill_name_desc(&skill_md, &directory);

        let (id, repo_owner, repo_name, repo_branch, readme_url) =
            build_repo_info_from_lock(&agents_lock, &directory);

        let content_hash = SkillService::compute_dir_hash(&ssot_path).ok();

        let skill = InstalledSkill {
            id,
            name,
            description,
            directory,
            repo_owner,
            repo_name,
            repo_branch,
            readme_url,
            apps,
            installed_at: chrono::Utc::now().timestamp(),
            content_hash,
            updated_at: 0,
        };

        db.save_skill(&skill)?;
        count += 1;
    }

    let _ = db.set_setting("skills_ssot_migration_snapshot", "");

    log::info!("Skills migration finished, {count} in total");

    Ok(count)
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn skill_state_lock_allows_snapshots_but_excludes_writers() {
        let first_reader = skill_state_read_guard();
        let second_reader = skill_state_read_guard();
        assert!(
            skill_state_lock().try_write().is_err(),
            "a Skill mutation must wait for every snapshot reader"
        );

        drop(second_reader);
        drop(first_reader);
        assert!(skill_state_lock().try_write().is_ok());
    }

    /// Build a ZIP that mimics a GitHub archive: one `repo-main/` root directory
    /// containing malicious entries that escape with `../`.
    ///
    /// The two malicious entries exercise **different** gates and both are needed:
    /// - two levels of `../..`: negative net depth, rejected by `enclosed_name()` itself;
    /// - one level of `../`: non-negative net depth, **allowed** by `enclosed_name()` which keeps the
    ///   `..` verbatim, so only the component check after stripping root_name can block it.
    fn build_zip_with_traversal_entry() -> Vec<u8> {
        use std::io::Write;
        use zip::write::SimpleFileOptions;

        let mut buf = Vec::new();
        {
            let mut zip = zip::ZipWriter::new(std::io::Cursor::new(&mut buf));
            let opts = SimpleFileOptions::default();

            // Legitimate entry: extracted normally
            zip.start_file("repo-main/SKILL.md", opts).unwrap();
            zip.write_all(b"---\nname: ok\n---\n").unwrap();

            // Malicious entry A: rejected by enclosed_name()
            zip.start_file("repo-main/../../escaped.txt", opts).unwrap();
            zip.write_all(b"pwned").unwrap();

            // Malicious entry B: passes enclosed_name(), blocked by the component check
            zip.start_file("repo-main/../escaped-one-level.txt", opts)
                .unwrap();
            zip.write_all(b"pwned").unwrap();

            zip.finish().unwrap();
        }
        buf
    }

    #[test]
    fn validate_repo_ref_accepts_real_world_coordinates() {
        // A legitimate branch name may contain `/`; traversal protection must not ban them all
        for branch in [
            "main",
            "master",
            "HEAD",
            "feature/new-thing",
            "release/v1.2.3",
            "fix-123",
            "user.name/topic",
        ] {
            assert!(
                SkillService::validate_repo_ref("farion1231", "cc-switch", branch).is_ok(),
                "must accept branch: {branch:?}"
            );
        }
        assert!(SkillService::validate_repo_ref("a", "b.c_d-e", "main").is_ok());
    }

    #[test]
    fn validate_repo_ref_accepts_the_empty_branch_sentinel() {
        // An empty branch and "HEAD" are the same sentinel in download_repo: the branch candidate table
        // skips both and tries main / master, so they never reach a URL. If validation treated the empty
        // string as invalid, existing skill_repos rows (column default 'main', empty string not banned)
        // would fail with INVALID_REPO_REF on the first line of download_repo and the whole skill panel
        // would list nothing - the two frontend `repo.branch || "main"` expressions assume it is usable.
        assert!(
            SkillService::validate_repo_ref("farion1231", "cc-switch", "").is_ok(),
            "the empty-branch sentinel must stay usable"
        );
    }

    #[test]
    fn validate_repo_ref_rejects_url_hijacking_branches() {
        // This is the core case: branch is spliced into the archive URL, URL parsing collapses dot
        // segments, and the target moves from /archive/refs/heads/ to an attacker-uploadable release asset.
        for branch in [
            "../../../releases/download/v1/evil",
            "..",
            "../x",
            "a/../../b",
            "a/./b",
            "..\\..\\releases\\download\\v1\\evil",
            "/leading",
            "trailing/",
            "double//slash",
            "with space",
            "frag#ment",
            "pct%2e%2e",
            "ref@{0}",
            "seg.lock",
            ".hidden/x",
        ] {
            assert!(
                SkillService::validate_repo_ref("owner", "repo", branch).is_err(),
                "must reject branch: {branch:?}"
            );
        }
        for (owner, name) in [
            ("..", "repo"),
            ("own/er", "repo"),
            ("owner", ".."),
            ("owner", "re/po"),
            ("owner", "re po"),
            ("", "repo"),
            ("owner", ""),
        ] {
            assert!(
                SkillService::validate_repo_ref(owner, name, "main").is_err(),
                "must reject coordinates: {owner:?}/{name:?}"
            );
        }
    }

    #[test]
    fn assert_github_archive_url_pins_host_and_path() {
        let ok = "https://github.com/owner/repo/archive/refs/heads/main.zip";
        assert!(SkillService::assert_github_archive_url(ok, "owner", "repo").is_ok());

        // The exit assertion must block a target redirected to a release asset
        for bad in [
            "https://github.com/owner/repo/releases/download/v1/evil.zip",
            "https://evil.example/owner/repo/archive/refs/heads/main.zip",
            "http://github.com/owner/repo/archive/refs/heads/main.zip",
            "https://github.com/other/repo/archive/refs/heads/main.zip",
        ] {
            assert!(
                SkillService::assert_github_archive_url(bad, "owner", "repo").is_err(),
                "must reject url: {bad}"
            );
        }
    }

    #[test]
    fn skill_source_fallback_only_accepts_transient_failures() {
        for status in [408, 429, 500, 502, 503, 599] {
            assert_eq!(
                RepoTransferFailure::classify_status(status),
                RepoTransferFailureClass::RetryableTransport,
                "status {status} should enter the one trusted fallback stage"
            );
        }
        assert_eq!(
            RepoTransferFailure::classify_status(404),
            RepoTransferFailureClass::NotFound
        );
        for status in [400, 401, 403, 407, 409, 422] {
            assert_eq!(
                RepoTransferFailure::classify_status(status),
                RepoTransferFailureClass::Terminal,
                "status {status} must preserve the primary error"
            );
        }
    }

    #[test]
    fn jsdelivr_urls_pin_https_hosts_and_encode_branch_segments() {
        let flat =
            SkillService::build_jsdelivr_manifest_url("anthropics", "skills", "feature/review")
                .expect("valid metadata URL");
        assert_eq!(flat.scheme(), "https");
        assert_eq!(flat.host_str(), Some("cdn.jsdelivr.net"));
        assert!(flat
            .as_str()
            .contains("skills@feature%2Freview/+private-json"));

        let file = SkillService::build_jsdelivr_file_url(
            "anthropics",
            "skills",
            "feature/review",
            "skills/code review/SKILL.md",
        )
        .expect("valid file URL");
        assert_eq!(file.scheme(), "https");
        assert_eq!(file.host_str(), Some("cdn.jsdelivr.net"));
        assert!(file.as_str().contains("skills@feature%2Freview"));
        assert!(file.as_str().ends_with("skills/code%20review/SKILL.md"));

        let hostile = url::Url::parse("https://evil.example/gh/a/b@main/x").unwrap();
        assert!(SkillService::assert_jsdelivr_response_url(&hostile, "cdn.jsdelivr.net").is_err());
    }

    #[test]
    fn jsdelivr_manifest_paths_are_portable_relative_files() {
        assert_eq!(
            SkillService::normalize_jsdelivr_file_path("/skills/pdf/SKILL.md").expect("safe path"),
            PathBuf::from("skills").join("pdf").join("SKILL.md")
        );
        for unsafe_path in [
            "skills/pdf/SKILL.md",
            "/../secret",
            "/skills/../../secret",
            "//server/share",
            "/skills\\secret",
            "/skills//SKILL.md",
            "/skills/NUL.txt",
            "/skills/trailing. ",
            "/skills/a?b",
        ] {
            assert!(
                SkillService::normalize_jsdelivr_file_path(unsafe_path).is_err(),
                "must reject {unsafe_path:?}"
            );
        }
    }

    #[test]
    fn jsdelivr_manifest_and_file_integrity_are_fail_closed() {
        let payload = b"---\nname: pdf\n---\n".to_vec();
        let hash = BASE64_STANDARD.encode(Sha256::digest(&payload));
        let repo = SkillRepo {
            owner: "anthropics".to_string(),
            name: "skills".to_string(),
            branch: "main".to_string(),
            enabled: true,
        };
        let manifest = JsdelivrFlatResponse {
            version: Some("main".to_string()),
            files: vec![JsdelivrFlatFile {
                name: "/skills/pdf/SKILL.md".to_string(),
                hash,
                size: payload.len() as u64,
            }],
        };
        let mut files = SkillService::validate_jsdelivr_manifest(&repo, "main", manifest)
            .expect("valid manifest");
        let file = files.pop().expect("one file");
        let (path, verified) = SkillService::verify_jsdelivr_file_bytes(file, payload.clone())
            .expect("matching bytes");
        assert_eq!(path, PathBuf::from("skills/pdf/SKILL.md"));
        assert_eq!(verified, payload);

        let bad_size = ValidatedMirrorFile {
            path: PathBuf::from("SKILL.md"),
            url: SkillService::build_jsdelivr_file_url("anthropics", "skills", "main", "SKILL.md")
                .unwrap(),
            expected_branch: "main".to_string(),
            expected_hash: Sha256::digest(b"x").to_vec(),
            expected_size: 2,
        };
        assert!(SkillService::verify_jsdelivr_file_bytes(bad_size, b"x".to_vec()).is_err());

        let bad_hash = ValidatedMirrorFile {
            path: PathBuf::from("SKILL.md"),
            url: SkillService::build_jsdelivr_file_url("anthropics", "skills", "main", "SKILL.md")
                .unwrap(),
            expected_branch: "main".to_string(),
            expected_hash: vec![0; 32],
            expected_size: 1,
        };
        assert!(SkillService::verify_jsdelivr_file_bytes(bad_hash, b"x".to_vec()).is_err());

        let mut branch_headers = reqwest::header::HeaderMap::new();
        branch_headers.insert("x-jsd-version", "main".parse().unwrap());
        branch_headers.insert("x-jsd-version-type", "branch".parse().unwrap());
        assert!(SkillService::assert_jsdelivr_branch_headers(&branch_headers, "main").is_ok());
        branch_headers.insert("x-jsd-version-type", "version".parse().unwrap());
        assert!(SkillService::assert_jsdelivr_branch_headers(&branch_headers, "main").is_err());

        let wrong_snapshot = JsdelivrFlatResponse {
            version: Some("master".to_string()),
            files: vec![JsdelivrFlatFile {
                name: "/SKILL.md".to_string(),
                hash: BASE64_STANDARD.encode(Sha256::digest(b"x")),
                size: 1,
            }],
        };
        assert!(SkillService::validate_jsdelivr_manifest(&repo, "main", wrong_snapshot).is_err());
    }

    #[test]
    fn build_skill_doc_url_drops_illegal_coordinates() {
        assert_eq!(
            SkillService::build_skill_doc_url("owner", "repo", "main", "a/SKILL.md").as_deref(),
            Some("https://github.com/owner/repo/blob/main/a/SKILL.md")
        );
        // readme_url is opened directly by the frontend openExternal, so invalid coordinates must produce no link
        assert!(
            SkillService::build_skill_doc_url("owner", "repo", "../../../issues", "x").is_none()
        );
    }

    #[test]
    fn copy_entry_within_budget_stops_before_exceeding_the_limit() {
        // The budget accumulates chunk by chunk and aborts without writing more once exceeded - the size
        // declared by a zip bomb cannot be trusted, so the decision uses the bytes actually read.
        let mut total = MAX_ARCHIVE_TOTAL_BYTES - 8;
        let mut reader = std::io::Cursor::new(vec![7u8; 64]);
        let mut writer: Vec<u8> = Vec::new();

        let err = SkillService::copy_entry_within_budget(&mut reader, &mut writer, &mut total)
            .expect_err("must reject once the budget is exhausted");
        assert!(
            err.to_string().contains("ARCHIVE_TOO_LARGE"),
            "unexpected error: {err}"
        );
        assert!(
            writer.is_empty(),
            "nothing may be written once the chunk would exceed the budget"
        );

        // Writes complete normally when the budget is sufficient
        let mut total = 0u64;
        let mut reader = std::io::Cursor::new(vec![7u8; 64]);
        let mut writer: Vec<u8> = Vec::new();
        SkillService::copy_entry_within_budget(&mut reader, &mut writer, &mut total)
            .expect("within budget");
        assert_eq!(writer.len(), 64);
        assert_eq!(total, 64);
    }

    #[test]
    fn extract_repo_archive_rejects_too_many_entries() {
        use std::io::Write;
        use zip::write::SimpleFileOptions;

        let mut buf = Vec::new();
        {
            let mut zip = zip::ZipWriter::new(std::io::Cursor::new(&mut buf));
            let opts = SimpleFileOptions::default();
            for i in 0..(MAX_ARCHIVE_ENTRIES + 1) {
                zip.start_file(format!("repo-main/f{i}"), opts).unwrap();
                zip.write_all(b"x").unwrap();
            }
            zip.finish().unwrap();
        }

        let temp = tempdir().expect("tempdir");
        let archive = zip::ZipArchive::new(std::io::Cursor::new(buf)).expect("archive parses");
        let err = SkillService::extract_repo_archive(archive, temp.path())
            .expect_err("entry count over the limit must be rejected");
        assert!(
            err.to_string().contains("ARCHIVE_TOO_MANY_ENTRIES"),
            "unexpected error: {err}"
        );
    }

    #[test]
    fn extract_repo_archive_rejects_path_traversal_entries() {
        let temp = tempdir().expect("tempdir");
        // dest sits one level deeper so that escaping one or two levels still lands inside temp and is detectable
        let dest = temp.path().join("nested").join("dest");
        fs::create_dir_all(&dest).expect("create dest");

        let bytes = build_zip_with_traversal_entry();
        let archive = zip::ZipArchive::new(std::io::Cursor::new(bytes)).expect("archive parses");

        SkillService::extract_repo_archive(archive, &dest).expect("extract must not fail");

        // The legitimate entry is written normally
        assert!(
            dest.join("SKILL.md").is_file(),
            "legitimate entry should be extracted"
        );
        // Two-level escape: must not be written outside dest
        assert!(
            !temp.path().join("escaped.txt").exists(),
            "zip-slip entry must not escape dest (temp root)"
        );
        assert!(
            !temp.path().join("nested").join("escaped.txt").exists(),
            "zip-slip entry must not escape dest (parent dir)"
        );
        // One-level escape: the kind enclosed_name() allows, which the component check must block
        assert!(
            !temp
                .path()
                .join("nested")
                .join("escaped-one-level.txt")
                .exists(),
            "single-`..` entry must not escape dest (enclosed_name allows it)"
        );
    }

    #[test]
    fn extract_repo_archive_skips_a_symlink_that_contains_itself() {
        // `dir/link -> ..` resolves to the archive root: it **passes** the "target must be inside base"
        // check because the target is base itself. Without the second self-containment check,
        // copy_dir_recursive(base, base/dir/link) would copy the root into its own subdirectory, seeing
        // the freshly written copy at every level until PATH_MAX ends it with an IO error.
        use std::io::Write;
        use zip::write::SimpleFileOptions;

        let mut buf = Vec::new();
        {
            let mut zip = zip::ZipWriter::new(std::io::Cursor::new(&mut buf));
            let opts = SimpleFileOptions::default();
            zip.start_file("repo-main/SKILL.md", opts).unwrap();
            zip.write_all(b"---\nname: t\ndescription: d\n---\n")
                .unwrap();
            zip.add_directory("repo-main/dir/", opts).unwrap();
            zip.add_symlink("repo-main/dir/link", "..", opts).unwrap();
            zip.finish().unwrap();
        }

        let temp = tempdir().expect("tempdir");
        let dest = temp.path().join("dest");
        fs::create_dir_all(&dest).expect("create dest");
        let archive = zip::ZipArchive::new(std::io::Cursor::new(buf)).expect("archive parses");

        SkillService::extract_repo_archive(archive, &dest)
            .expect("a self-containing symlink must be skipped, not blow up the extraction");

        assert!(
            dest.join("SKILL.md").is_file(),
            "legitimate entries must still be extracted"
        );
        assert!(
            !dest.join("dir").join("link").exists(),
            "a symlink whose target contains the link itself must not be materialized"
        );
    }

    #[test]
    fn symlink_materialization_is_charged_to_the_archive_budget() {
        // Materializing a symlink happens in the second pass, on a different code path from the
        // extraction loop. Without sharing one budget, "one big file + N symlinks to it" could write N
        // times the bytes while the cap still looked compliant. The budget is preset near the cap here to prove materialization is billed.
        let temp = tempdir().expect("tempdir");
        let base = temp.path().join("base");
        fs::create_dir_all(base.join("payload")).expect("create payload dir");
        fs::write(base.join("payload").join("big.bin"), vec![b'x'; 4096]).expect("write payload");

        let symlinks = vec![(base.join("copy"), "payload".to_string())];
        let mut total_bytes = MAX_ARCHIVE_TOTAL_BYTES - 1024;

        let err = SkillService::resolve_symlinks_in_dir(&base, &symlinks, &mut total_bytes)
            .expect_err("materializing 4 KiB with 1 KiB of budget left must fail");
        assert!(
            err.to_string().contains("ARCHIVE_TOO_LARGE"),
            "unexpected error: {err}"
        );
    }

    #[test]
    fn symlink_guard_sees_through_unnormalized_link_paths() {
        // `enclosed_name()` does not resolve `..`, so link_path may look like `e/../d/self`. If the guard
        // compared it literally against the canonicalized target it would decide "not contained" at the
        // first component (`e` vs `d`) - yet that location physically sits inside `d`, and copying `d` into it is exactly a recursive self copy.
        let temp = tempdir().expect("tempdir");
        let base = temp.path().join("base");
        fs::create_dir_all(base.join("d").join("sub")).expect("create d");
        fs::create_dir_all(base.join("e")).expect("create e");

        let link_path = base.join("e").join("..").join("d").join("self");
        let symlinks = vec![(link_path, ".".to_string())];
        let mut total_bytes = 0u64;

        SkillService::resolve_symlinks_in_dir(&base, &symlinks, &mut total_bytes)
            .expect("a self-containing symlink must be skipped, not blow up the extraction");

        assert!(
            !base.join("d").join("self").exists(),
            "a link that physically lives inside its own target must not be materialized"
        );
    }

    /// A reader that records how many bytes were actually consumed.
    ///
    /// Asserting on the return value alone is not enough: the function already ends with a length check,
    /// so removing the `take` cap would still return `None` and the assertion would still pass - while
    /// the damage of a bomb happens during reading, not in the return value. Only observing the consumed amount really pins down "the read is bounded".
    struct CountingReader {
        remaining: u64,
        consumed: u64,
    }

    impl std::io::Read for CountingReader {
        fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
            if self.remaining == 0 {
                return Ok(0);
            }
            let n = buf.len().min(self.remaining as usize);
            buf[..n].fill(b'a');
            self.remaining -= n as u64;
            self.consumed += n as u64;
            Ok(n)
        }
    }

    #[test]
    fn read_symlink_target_is_bounded_and_charged() {
        // An entry flagged as a symlink whose decompressed stream is huge: the make_reader of zip 2.4.2
        // does not truncate at the declared uncompressed_size, so without a cap it is read fully into memory.
        let mut oversized = CountingReader {
            remaining: 8 * 1024 * 1024,
            consumed: 0,
        };
        let mut total_bytes = 0u64;

        let target = SkillService::read_symlink_target(&mut oversized, &mut total_bytes)
            .expect("an oversized target must be skipped, not raise");
        assert!(
            target.is_none(),
            "a target longer than a path can plausibly be must be rejected"
        );
        assert_eq!(total_bytes, 0, "a rejected target must not be charged");
        assert!(
            oversized.consumed <= MAX_SYMLINK_TARGET_BYTES + 1,
            "the read must stop at the cap instead of draining the stream, consumed {}",
            oversized.consumed
        );

        // A normal target is read as usual and billed
        let mut normal = std::io::Cursor::new(b"../shared".to_vec());
        let target = SkillService::read_symlink_target(&mut normal, &mut total_bytes)
            .expect("a normal target must be read");
        assert_eq!(target.as_deref(), Some("../shared"));
        assert_eq!(total_bytes, 9);
    }

    #[test]
    fn directory_materialization_is_charged_to_the_archive_budget() {
        // An archive of nothing but empty directories writes zero content bytes. Without billing
        // directories, the second symlink pass could grow the count exponentially with depth while the budget reading stayed 0.
        let temp = tempdir().expect("tempdir");
        let src = temp.path().join("src");
        fs::create_dir_all(src.join("a").join("b")).expect("create tree");

        let mut total_bytes = MAX_ARCHIVE_TOTAL_BYTES - DIRECTORY_BUDGET_COST;
        let err =
            SkillService::copy_dir_within_budget(&src, &temp.path().join("dest"), &mut total_bytes)
                .expect_err("materializing directories past the limit must fail");
        assert!(
            err.to_string().contains("ARCHIVE_TOO_LARGE"),
            "unexpected error: {err}"
        );
    }

    #[test]
    fn create_dir_all_charges_every_directory_it_creates() {
        // `create_dir_all` can create every missing parent at once, so an entry named `a/a/.../a/f.txt`
        // implicitly creates hundreds of levels. Billing per call would badly underestimate that.
        let temp = tempdir().expect("tempdir");
        let deep = temp.path().join("a").join("b").join("c");

        // The budget only covers two levels, so creating three must be blocked
        let mut total_bytes = MAX_ARCHIVE_TOTAL_BYTES - 2 * DIRECTORY_BUDGET_COST;
        let err = SkillService::create_dir_all_within_budget(&deep, &mut total_bytes)
            .expect_err("creating more directories than the budget allows must fail");
        assert!(
            err.to_string().contains("ARCHIVE_TOO_LARGE"),
            "unexpected error: {err}"
        );
        assert!(
            !deep.exists(),
            "nothing must be created once the budget is exceeded"
        );
    }

    #[test]
    fn extract_local_zip_leaves_no_partial_directory_when_it_fails() {
        use std::io::Write;
        use zip::write::SimpleFileOptions;

        // scratch is fed to this one extraction only: the temp directories of concurrent tests cannot
        // land here, so the "must be empty" assertion observes exactly this extraction's residue
        let holder = tempdir().expect("tempdir");
        let scratch = tempdir().expect("tempdir");

        let mut buf = Vec::new();
        {
            let mut zip = zip::ZipWriter::new(std::io::Cursor::new(&mut buf));
            let opts = SimpleFileOptions::default();
            // `x` is written as a file first and then required to act as a directory - create_dir_all must
            // fail, and it fails after the temporary directory was created and already written to
            zip.start_file("x", opts).unwrap();
            zip.write_all(b"i am a file").unwrap();
            zip.start_file("x/y", opts).unwrap();
            zip.write_all(b"and my parent is not a directory").unwrap();
            zip.finish().unwrap();
        }
        let zip_path = holder.path().join("collide.zip");
        fs::write(&zip_path, &buf).expect("write zip");

        let result = SkillService::extract_local_zip_in(&zip_path, scratch.path());

        assert!(
            result.is_err(),
            "the fixture must actually fail after the temp dir exists"
        );
        let leftovers: Vec<_> = fs::read_dir(scratch.path())
            .expect("read scratch")
            .filter_map(Result::ok)
            .map(|entry| entry.path())
            .collect();
        assert!(
            leftovers.is_empty(),
            "a failed extraction must not leave a partial directory behind: {leftovers:?}"
        );
    }

    #[test]
    fn extract_local_zip_hands_back_a_guard_that_owns_the_tree() {
        use std::io::Write;
        use zip::write::SimpleFileOptions;

        // After a successful extraction the caller still walks a long chain of `?` for scanning / getting
        // the SSOT / copying / writing the database / syncing. It used to return a bare PathBuf with
        // cleanup relying on hand-written remove_dir_all at every exit - more than one had forgotten
        // (copy_dir_recursive and save_skill in install_from_zip, copy_dir_recursive in update_skill, scan_dir_recursive in fetch_repo_skills).
        let holder = tempdir().expect("tempdir");
        let scratch = tempdir().expect("tempdir");

        let mut buf = Vec::new();
        {
            let mut zip = zip::ZipWriter::new(std::io::Cursor::new(&mut buf));
            let opts = SimpleFileOptions::default();
            zip.start_file("s/SKILL.md", opts).unwrap();
            zip.write_all(b"# skill").unwrap();
            zip.finish().unwrap();
        }
        let zip_path = holder.path().join("ok.zip");
        fs::write(&zip_path, &buf).expect("write zip");

        let extracted = SkillService::extract_local_zip_in(&zip_path, scratch.path())
            .expect("extract must succeed");
        assert!(
            extracted.path().join("s").join("SKILL.md").exists(),
            "the fixture must actually extract something worth cleaning up"
        );

        // Simulate the caller returning early at any downstream `?`
        drop(extracted);

        let leftovers: Vec<_> = fs::read_dir(scratch.path())
            .expect("read scratch")
            .filter_map(Result::ok)
            .map(|entry| entry.path())
            .collect();
        assert!(
            leftovers.is_empty(),
            "dropping the extraction result must take the whole tree with it: {leftovers:?}"
        );
    }

    #[test]
    fn extract_local_zip_rejects_dot_dot_entries() {
        use std::io::Write;
        use zip::write::SimpleFileOptions;

        let mut buf = Vec::new();
        {
            let mut zip = zip::ZipWriter::new(std::io::Cursor::new(&mut buf));
            let opts = SimpleFileOptions::default();
            zip.add_directory("d/", opts).unwrap();
            zip.add_directory("e/", opts).unwrap();
            // Non-negative net depth, allowed by enclosed_name(); the unresolved `..` would land it inside d
            zip.start_file("e/../d/leaked.txt", opts).unwrap();
            zip.write_all(b"x").unwrap();
            zip.finish().unwrap();
        }

        let temp = tempdir().expect("tempdir");
        let zip_path = temp.path().join("dots.zip");
        fs::write(&zip_path, &buf).expect("write zip");

        let extracted = SkillService::extract_local_zip(&zip_path).expect("extract must not fail");
        assert!(
            !extracted.path().join("d").join("leaked.txt").exists(),
            "an entry with an unresolved `..` must be skipped, not silently relocated"
        );
    }

    #[test]
    fn extract_local_zip_rejects_too_many_entries() {
        // A local ZIP uses the other extractor, and the entry cap used to exist only on the remote archive path.
        use std::io::Write;
        use zip::write::SimpleFileOptions;

        let mut buf = Vec::new();
        {
            let mut zip = zip::ZipWriter::new(std::io::Cursor::new(&mut buf));
            let opts = SimpleFileOptions::default();
            for i in 0..(MAX_ARCHIVE_ENTRIES + 1) {
                zip.start_file(format!("f{i}"), opts).unwrap();
                zip.write_all(b"x").unwrap();
            }
            zip.finish().unwrap();
        }

        let temp = tempdir().expect("tempdir");
        let zip_path = temp.path().join("bomb.zip");
        fs::write(&zip_path, &buf).expect("write zip");

        let err = SkillService::extract_local_zip(&zip_path)
            .expect_err("entry count over the limit must be rejected for local ZIPs too");
        assert!(
            err.to_string().contains("ARCHIVE_TOO_MANY_ENTRIES"),
            "unexpected error: {err}"
        );
    }

    fn write_skill(dir: &Path, name: &str) {
        fs::create_dir_all(dir).expect("create skill dir");
        fs::write(
            dir.join("SKILL.md"),
            format!("---\nname: {name}\ndescription: Test skill\n---\n"),
        )
        .expect("write SKILL.md");
    }

    #[test]
    #[serial_test::serial]
    fn pi_skill_state_follows_native_directory_presence() {
        let temp = tempdir().expect("tempdir");
        let _home = TestHomeGuard::set(temp.path());
        let _pi_dir = crate::pi_config::test_support::TestAgentDir::new();

        let db = std::sync::Arc::new(Database::memory().expect("memory db"));
        let mut skill = poisoned_skill("owner/repo:skill", "test-skill");
        skill.apps.pi = true;
        db.save_skill(&skill).expect("save skill");

        let installed = SkillService::get_all_installed(&db).expect("read skills");
        assert!(!installed[0].apps.pi, "a DB flag must not activate Pi");

        write_skill(
            &SkillService::get_ssot_dir().unwrap().join("test-skill"),
            "test",
        );
        SkillService::toggle_app(&db, &skill.id, &AppType::Pi, true).expect("enable Pi skill");
        assert!(SkillService::get_all_installed(&db).unwrap()[0].apps.pi);

        SkillService::toggle_app(&db, &skill.id, &AppType::Pi, false).expect("disable Pi skill");
        assert!(!SkillService::get_all_installed(&db).unwrap()[0].apps.pi);
    }

    #[test]
    #[serial_test::serial]
    fn pi_skill_toggle_preserves_a_same_name_external_directory() {
        let temp = tempdir().expect("tempdir");
        let _home = TestHomeGuard::set(temp.path());
        let _pi_dir = crate::pi_config::test_support::TestAgentDir::new();

        let db = std::sync::Arc::new(Database::memory().expect("memory db"));
        let skill = poisoned_skill("owner/repo:skill", "test-skill");
        db.save_skill(&skill).expect("save skill");
        write_skill(
            &SkillService::get_ssot_dir().unwrap().join("test-skill"),
            "managed",
        );
        let pi_skill = SkillService::get_app_skills_dir(&AppType::Pi)
            .unwrap()
            .join("test-skill");
        write_skill(&pi_skill, "external");

        let enable = SkillService::toggle_app(&db, &skill.id, &AppType::Pi, true);
        assert!(
            enable.is_err(),
            "enable must not overwrite external content"
        );
        assert!(fs::read_to_string(pi_skill.join("SKILL.md"))
            .unwrap()
            .contains("external"));

        let disable = SkillService::toggle_app(&db, &skill.id, &AppType::Pi, false);
        assert!(disable.is_err(), "disable must not delete external content");
        assert!(pi_skill.join("SKILL.md").exists());
    }

    #[test]
    #[serial_test::serial]
    fn pi_install_conflict_is_rejected_before_persisting() {
        let temp = tempdir().expect("tempdir");
        let _home = TestHomeGuard::set(temp.path());
        let _pi_dir = crate::pi_config::test_support::TestAgentDir::new();

        let db = std::sync::Arc::new(Database::memory().expect("memory db"));
        let skill = poisoned_skill("owner/repo:skill", "test-skill");
        write_skill(
            &SkillService::get_ssot_dir().unwrap().join("test-skill"),
            "managed",
        );
        let pi_skill = SkillService::get_app_skills_dir(&AppType::Pi)
            .unwrap()
            .join("test-skill");
        write_skill(&pi_skill, "external");

        let result = SkillService::persist_and_sync_new_skill(&db, &skill, &AppType::Pi);

        assert!(result.is_err());
        assert!(db
            .get_installed_skill(&skill.id)
            .expect("read skill")
            .is_none());
        assert!(fs::read_to_string(pi_skill.join("SKILL.md"))
            .expect("read external skill")
            .contains("external"));
    }

    #[test]
    fn managed_pi_copy_refreshes_from_its_expected_old_content() {
        let temp = tempdir().expect("tempdir");
        let source = temp.path().join("source");
        let destination = temp.path().join("pi-skill");
        write_skill(&source, "old");
        SkillService::copy_dir_recursive(&source, &destination).expect("seed managed copy");

        let deployment =
            SkillService::inspect_pi_skill_destination(&source, &destination, "test-skill")
                .expect("inspect managed copy")
                .expect("managed deployment");

        fs::remove_dir_all(&source).expect("replace old source");
        write_skill(&source, "new");
        SkillService::refresh_pi_skill_destination(
            &source,
            &destination,
            "test-skill",
            &deployment,
        )
        .expect("refresh managed copy");

        assert!(fs::read_to_string(destination.join("SKILL.md"))
            .expect("read refreshed copy")
            .contains("name: new"));
    }

    #[test]
    fn managed_pi_copy_rejects_hidden_native_changes() {
        let temp = tempdir().expect("tempdir");
        let source = temp.path().join("source");
        let destination = temp.path().join("pi-skill");
        write_skill(&source, "old");
        SkillService::copy_dir_recursive(&source, &destination).expect("seed managed copy");
        let deployment =
            SkillService::inspect_pi_skill_destination(&source, &destination, "test-skill")
                .expect("inspect managed copy")
                .expect("managed deployment");

        fs::write(destination.join(".env"), "user-owned").expect("add hidden native file");
        fs::remove_dir_all(&source).expect("replace old source");
        write_skill(&source, "new");

        let result = SkillService::refresh_pi_skill_destination(
            &source,
            &destination,
            "test-skill",
            &deployment,
        );
        assert!(
            result.is_err(),
            "hidden native changes must block replacement"
        );
        assert_eq!(
            fs::read_to_string(destination.join(".env")).expect("read hidden native file"),
            "user-owned"
        );
    }

    #[test]
    #[serial_test::serial]
    fn importing_a_native_pi_skill_returns_its_derived_active_state() {
        let temp = tempdir().expect("tempdir");
        let _home = TestHomeGuard::set(temp.path());
        let _pi_dir = crate::pi_config::test_support::TestAgentDir::new();
        let db = std::sync::Arc::new(Database::memory().expect("memory db"));
        let pi_skill = SkillService::get_app_skills_dir(&AppType::Pi)
            .unwrap()
            .join("native-skill");
        write_skill(&pi_skill, "native");

        let imported = SkillService::import_from_apps(
            &db,
            vec![ImportSkillSelection {
                directory: "native-skill".to_string(),
                apps: SkillApps::default(),
            }],
        )
        .expect("import native Pi skill");

        assert_eq!(imported.len(), 1);
        assert!(imported[0].apps.pi);
        assert!(SkillService::get_all_installed(&db).unwrap()[0].apps.pi);
    }

    #[test]
    #[serial_test::serial]
    fn uninstall_preserves_an_external_pi_skill_and_removes_the_managed_record() {
        let temp = tempdir().expect("tempdir");
        let _home = TestHomeGuard::set(temp.path());
        let _pi_dir = crate::pi_config::test_support::TestAgentDir::new();

        let db = std::sync::Arc::new(Database::memory().expect("memory db"));
        let skill = poisoned_skill("owner/repo:skill", "test-skill");
        db.save_skill(&skill).expect("save skill");
        let source = SkillService::get_ssot_dir().unwrap().join("test-skill");
        write_skill(&source, "managed");
        let pi_skill = SkillService::get_app_skills_dir(&AppType::Pi)
            .unwrap()
            .join("test-skill");
        write_skill(&pi_skill, "external");

        let result = SkillService::uninstall(&db, &skill.id).expect("uninstall managed record");

        assert!(!source.exists(), "managed source must be removed");
        assert!(db
            .get_installed_skill(&skill.id)
            .expect("query skill")
            .is_none());
        assert_eq!(
            result.preserved_pi_path,
            Some(pi_skill.to_string_lossy().to_string())
        );
        assert!(result.pi_cleanup_incomplete);
        assert!(fs::read_to_string(pi_skill.join("SKILL.md"))
            .expect("read external skill")
            .contains("name: external"));
    }

    #[test]
    #[serial_test::serial]
    fn uninstall_does_not_remove_a_preserved_pi_skill_through_another_app() {
        let temp = tempdir().expect("tempdir");
        let _home = TestHomeGuard::set(temp.path());
        let _pi_dir =
            crate::pi_config::test_support::TestAgentDir::at(&temp.path().join(".claude"));

        let db = std::sync::Arc::new(Database::memory().expect("memory db"));
        let skill = poisoned_skill("owner/repo:skill", "test-skill");
        db.save_skill(&skill).expect("save skill");
        let source = SkillService::get_ssot_dir().unwrap().join("test-skill");
        write_skill(&source, "managed");
        let shared_skill = SkillService::get_app_skills_dir(&AppType::Pi)
            .unwrap()
            .join("test-skill");
        assert_eq!(
            shared_skill,
            SkillService::get_app_skills_dir(&AppType::Claude)
                .unwrap()
                .join("test-skill")
        );
        write_skill(&shared_skill, "external");

        let result = SkillService::uninstall(&db, &skill.id).expect("uninstall managed record");

        assert!(!source.exists(), "managed source must be removed");
        assert_eq!(
            result.preserved_pi_path,
            Some(shared_skill.to_string_lossy().to_string())
        );
        assert!(result.pi_cleanup_incomplete);
        assert!(fs::read_to_string(shared_skill.join("SKILL.md"))
            .expect("read shared external skill")
            .contains("name: external"));
        assert!(db
            .get_installed_skill(&skill.id)
            .expect("query skill")
            .is_none());
    }

    #[cfg(unix)]
    #[test]
    #[serial_test::serial]
    fn uninstall_preserves_a_dangling_pi_link_through_an_aliased_app_root() {
        use std::os::unix::fs::symlink;

        let temp = tempdir().expect("tempdir");
        let _home = TestHomeGuard::set(temp.path());
        let shared_agent = temp.path().join("shared-agent");
        fs::create_dir_all(shared_agent.join("skills")).expect("create shared skills root");
        symlink(&shared_agent, temp.path().join(".claude")).expect("alias Claude root");
        let _pi_dir = crate::pi_config::test_support::TestAgentDir::at(&shared_agent);

        let db = std::sync::Arc::new(Database::memory().expect("memory db"));
        let skill = poisoned_skill("owner/repo:skill", "test-skill");
        db.save_skill(&skill).expect("save skill");
        let source = SkillService::get_ssot_dir().unwrap().join("test-skill");
        write_skill(&source, "managed");
        let pi_skill = shared_agent.join("skills").join("test-skill");
        symlink(temp.path().join("missing-target"), &pi_skill).expect("dangling Pi link");

        let result = SkillService::uninstall(&db, &skill.id).expect("uninstall managed record");

        assert_eq!(
            result.preserved_pi_path,
            Some(pi_skill.to_string_lossy().to_string())
        );
        assert!(result.pi_cleanup_incomplete);
        assert!(
            SkillService::is_symlink(&pi_skill),
            "the aliased application cleanup must not remove the preserved link"
        );
        assert!(!source.exists());
    }

    #[test]
    fn pi_uninstall_rechecks_a_destination_created_after_preflight() {
        let temp = tempdir().expect("tempdir");
        let source = temp.path().join("source");
        let destination = temp.path().join("pi-skill");
        write_skill(&source, "managed");
        assert!(
            SkillService::inspect_pi_skill_destination(&source, &destination, "test-skill")
                .expect("inspect absent destination")
                .is_none()
        );

        SkillService::copy_dir_recursive(&source, &destination)
            .expect("simulate concurrent Pi sync");
        SkillService::remove_verified_pi_destination(&source, &destination, "test-skill")
            .expect("remove destination created after preflight");

        assert!(!destination.exists());
    }

    #[test]
    #[serial_test::serial]
    fn uninstall_keeps_ssot_when_it_contains_a_preserved_pi_skill() {
        let temp = tempdir().expect("tempdir");
        let _home = TestHomeGuard::set(temp.path());

        let db = std::sync::Arc::new(Database::memory().expect("memory db"));
        let skill = poisoned_skill("owner/repo:skill", "test-skill");
        db.save_skill(&skill).expect("save skill");
        let source = SkillService::get_ssot_dir().unwrap().join("test-skill");
        write_skill(&source, "managed");
        let _pi_dir = crate::pi_config::test_support::TestAgentDir::at(&source.join("pi-agent"));
        let pi_skill = SkillService::get_app_skills_dir(&AppType::Pi)
            .unwrap()
            .join("test-skill");
        write_skill(&pi_skill, "external");

        let result = SkillService::uninstall(&db, &skill.id).expect("uninstall managed record");

        assert!(result.backup_path.is_none());
        assert_eq!(
            result.preserved_pi_path,
            Some(pi_skill.to_string_lossy().to_string())
        );
        assert!(result.pi_cleanup_incomplete);
        assert!(source.exists(), "the preserved path is nested under SSOT");
        assert!(pi_skill.exists(), "the external Pi skill must remain");
        assert!(db
            .get_installed_skill(&skill.id)
            .expect("query skill")
            .is_none());
    }

    #[test]
    #[serial_test::serial]
    fn uninstall_with_a_missing_source_does_not_backup_or_delete_the_pi_directory() {
        let temp = tempdir().expect("tempdir");
        let _home = TestHomeGuard::set(temp.path());
        let _pi_dir = crate::pi_config::test_support::TestAgentDir::new();

        let db = std::sync::Arc::new(Database::memory().expect("memory db"));
        let skill = poisoned_skill("owner/repo:skill", "test-skill");
        db.save_skill(&skill).expect("save skill");
        let pi_skill = SkillService::get_app_skills_dir(&AppType::Pi)
            .unwrap()
            .join("test-skill");
        write_skill(&pi_skill, "external");

        let result = SkillService::uninstall(&db, &skill.id).expect("uninstall missing source");

        assert!(result.backup_path.is_none());
        assert_eq!(
            result.preserved_pi_path,
            Some(pi_skill.to_string_lossy().to_string())
        );
        assert!(result.pi_cleanup_incomplete);
        assert!(pi_skill.join("SKILL.md").exists());
        assert!(db
            .get_installed_skill(&skill.id)
            .expect("query skill")
            .is_none());
    }

    #[test]
    #[serial_test::serial]
    fn uninstall_treats_an_aliased_pi_root_as_the_ssot() {
        let _location = StorageLocationGuard::set(SkillStorageLocation::Unified);
        let temp = tempdir().expect("tempdir");
        let _home = TestHomeGuard::set(temp.path());
        let _pi_dir =
            crate::pi_config::test_support::TestAgentDir::at(&temp.path().join(".agents"));

        let db = std::sync::Arc::new(Database::memory().expect("memory db"));
        let skill = poisoned_skill("owner/repo:skill", "test-skill");
        db.save_skill(&skill).expect("save skill");
        let source = SkillService::get_ssot_dir().unwrap().join("test-skill");
        write_skill(&source, "managed");

        let result = SkillService::uninstall(&db, &skill.id).expect("uninstall aliased skill");

        assert!(!source.exists());
        assert!(result.preserved_pi_path.is_none());
        assert!(!result.pi_cleanup_incomplete);
        assert!(db
            .get_installed_skill(&skill.id)
            .expect("query skill")
            .is_none());
    }

    #[test]
    #[serial_test::serial]
    fn uninstall_warns_when_the_pi_root_cannot_be_resolved() {
        let _location = StorageLocationGuard::set(SkillStorageLocation::CcSwitch);
        let temp = tempdir().expect("tempdir");
        let _home = TestHomeGuard::set(temp.path());
        let _pi_dir =
            crate::pi_config::test_support::TestAgentDir::at(Path::new("relative/pi-agent"));

        let db = std::sync::Arc::new(Database::memory().expect("memory db"));
        let skill = poisoned_skill("owner/repo:skill", "test-skill");
        db.save_skill(&skill).expect("save skill");
        let source = SkillService::get_ssot_dir().unwrap().join("test-skill");
        write_skill(&source, "managed");

        let result =
            SkillService::uninstall(&db, &skill.id).expect("uninstall with invalid Pi root");

        assert!(!source.exists());
        assert!(result.preserved_pi_path.is_none());
        assert!(result.pi_cleanup_incomplete);
        assert!(db
            .get_installed_skill(&skill.id)
            .expect("query skill")
            .is_none());
    }

    /// AI_MANAGER_TEST_HOME isolation guard (mutual exclusion between serial tests is provided by
    /// #[serial]; the guard only restores the original value after the test).
    struct TestHomeGuard(Option<std::ffi::OsString>);
    impl TestHomeGuard {
        fn set(home: &Path) -> Self {
            let guard = Self(std::env::var_os("AI_MANAGER_TEST_HOME"));
            std::env::set_var("AI_MANAGER_TEST_HOME", home);
            guard
        }
    }
    impl Drop for TestHomeGuard {
        fn drop(&mut self) {
            match self.0.take() {
                Some(value) => std::env::set_var("AI_MANAGER_TEST_HOME", value),
                None => std::env::remove_var("AI_MANAGER_TEST_HOME"),
            }
        }
    }

    struct StorageLocationGuard(SkillStorageLocation);
    impl StorageLocationGuard {
        fn set(location: SkillStorageLocation) -> Self {
            let previous = crate::settings::get_skill_storage_location();
            crate::settings::set_skill_storage_location(location)
                .expect("set test skill storage location");
            Self(previous)
        }
    }
    impl Drop for StorageLocationGuard {
        fn drop(&mut self) {
            let _ = crate::settings::set_skill_storage_location(self.0);
        }
    }

    fn poisoned_skill(id: &str, directory: &str) -> InstalledSkill {
        InstalledSkill {
            id: id.to_string(),
            name: "poisoned".to_string(),
            description: None,
            directory: directory.to_string(),
            repo_owner: None,
            repo_name: None,
            repo_branch: None,
            readme_url: None,
            apps: SkillApps::default(),
            installed_at: 0,
            content_hash: None,
            updated_at: 0,
        }
    }

    #[test]
    fn persist_updated_skill_metadata_uses_database_apps() {
        let db = Arc::new(Database::memory().expect("memory db"));
        let mut installed = poisoned_skill("owner/repo:skill", "skill");
        installed.name = "old name".to_string();
        installed.apps = SkillApps::only(&AppType::Claude);
        db.save_skill(&installed).expect("seed skill");

        // Simulate the user switching the skill from Claude to Codex during the download. The metadata
        // waiting to be written still carries the old apps snapshot from when the download started.
        let authoritative_apps = SkillApps::only(&AppType::Codex);
        db.update_skill_apps(&installed.id, &authoritative_apps)
            .expect("toggle apps");

        let mut updated_metadata = installed.clone();
        updated_metadata.name = "new name".to_string();
        updated_metadata.content_hash = Some("new hash".to_string());
        updated_metadata.updated_at = 42;

        let persisted = SkillService::persist_updated_skill_metadata(&db, &updated_metadata)
            .expect("persist metadata");

        assert_eq!(persisted.name, "new name");
        assert_eq!(persisted.content_hash.as_deref(), Some("new hash"));
        assert_eq!(persisted.updated_at, 42);
        assert_eq!(persisted.apps, authoritative_apps);
        assert_eq!(
            db.get_installed_skill(&installed.id)
                .expect("query skill")
                .expect("skill remains installed")
                .apps,
            authoritative_apps
        );
    }

    #[test]
    fn persist_updated_skill_metadata_does_not_restore_uninstalled_skill() {
        let db = Arc::new(Database::memory().expect("memory db"));
        let installed = poisoned_skill("owner/repo:skill", "skill");
        db.save_skill(&installed).expect("seed skill");

        // Simulate the uninstall completing during the download, after which the old update task tries to write.
        assert!(db.delete_skill(&installed.id).expect("uninstall skill"));

        let mut updated_metadata = installed.clone();
        updated_metadata.name = "downloaded update".to_string();
        let err = SkillService::persist_updated_skill_metadata(&db, &updated_metadata)
            .expect_err("an uninstalled skill must not be restored");

        assert!(
            err.to_string().contains("Skill no longer installed"),
            "unexpected error: {err}"
        );
        assert!(db
            .get_installed_skill(&installed.id)
            .expect("query skill")
            .is_none());
    }

    #[test]
    fn require_valid_directory_accepts_single_segment_names_only() {
        assert_eq!(
            SkillService::require_valid_directory("my-skill").expect("valid name"),
            "my-skill"
        );
        for bad in [
            "..",
            "../..",
            "../../etc",
            "a/b",
            "a\\b",
            "",
            ".hidden",
            "C:\\evil",
            "/etc",
        ] {
            assert!(
                SkillService::require_valid_directory(bad).is_err(),
                "must reject: {bad:?}"
            );
        }
    }

    #[test]
    fn local_hash_for_update_check_ignores_cached_hash_when_dir_missing() {
        // The scenario of restoring a database backup on a new machine: content_hash is still in the DB
        // while the SSOT directory is gone. The cache must be ignored and None returned so the skill
        // enters the update list and the files can be rebuilt; trusting the cache would show "up to date" and hide the missing state forever.
        let ssot = tempdir().expect("tempdir");
        assert_eq!(
            SkillService::local_hash_for_update_check(ssot.path(), "my-skill", Some("cached")),
            None
        );
    }

    #[test]
    fn local_hash_for_update_check_uses_cache_when_dir_exists() {
        let ssot = tempdir().expect("tempdir");
        fs::create_dir(ssot.path().join("my-skill")).expect("create skill dir");
        assert_eq!(
            SkillService::local_hash_for_update_check(ssot.path(), "my-skill", Some("cached")),
            Some(("cached".to_string(), false))
        );
    }

    #[test]
    fn local_hash_for_update_check_computes_and_backfills_when_cache_empty() {
        let ssot = tempdir().expect("tempdir");
        let dir = ssot.path().join("my-skill");
        fs::create_dir(&dir).expect("create skill dir");
        fs::write(dir.join("SKILL.md"), "---\nname: x\n---\n").expect("write skill");

        let expected = SkillService::compute_dir_hash(&dir).expect("hash");
        assert_eq!(
            SkillService::local_hash_for_update_check(ssot.path(), "my-skill", None),
            Some((expected, true))
        );
    }

    #[test]
    fn local_hash_for_update_check_keeps_cache_for_invalid_directory() {
        // An invalid directory cannot be joined safely, so no existence check is possible: reuse the
        // cache when present (preserving pre-fix behaviour), return None otherwise. It must not report
        // "updatable" for an invalid value, or clicking update would hard-error on the same check in update_skill.
        let ssot = tempdir().expect("tempdir");
        assert_eq!(
            SkillService::local_hash_for_update_check(ssot.path(), "../evil", Some("cached")),
            Some(("cached".to_string(), false))
        );
        assert_eq!(
            SkillService::local_hash_for_update_check(ssot.path(), "../evil", None),
            None
        );
    }

    #[test]
    #[serial_test::serial]
    fn restore_from_backup_rejects_traversal_directory_in_metadata() {
        let temp = tempdir().expect("tempdir");
        let _guard = TestHomeGuard::set(temp.path());

        // Hand-place a backup whose meta.json directory points outside the product SSOT.
        // "../../pwned-restore" would escape AppData/skills if it took effect.
        let backup_id = "20260727_120000_evil";
        let backup_dir = SkillService::get_backup_dir()
            .expect("backup dir")
            .join(backup_id);
        write_skill(&backup_dir.join("skill"), "evil");
        let metadata = SkillBackupMetadata {
            skill: poisoned_skill("owner/repo:evil", "../../pwned-restore"),
            backup_created_at: 0,
            source_path: "x".to_string(),
        };
        fs::write(
            backup_dir.join("meta.json"),
            serde_json::to_string_pretty(&metadata).expect("serialize metadata"),
        )
        .expect("write meta.json");

        let db = std::sync::Arc::new(Database::memory().expect("memory db"));
        let result = SkillService::restore_from_backup(&db, backup_id, &AppType::Claude);

        assert!(
            result.is_err(),
            "restore must reject a traversal directory from meta.json"
        );
        assert!(
            !temp.path().join("pwned-restore").exists(),
            "restore must not write outside the SSOT dir"
        );
    }

    #[test]
    #[serial_test::serial]
    fn remove_from_app_rejects_traversal_directory() {
        let temp = tempdir().expect("tempdir");
        let _guard = TestHomeGuard::set(temp.path());

        // Both the victim directory and the app skills directory are created first, so the unfixed code
        // really could delete it: app_dir = {home}/.claude/skills, and "../../victim-remove" resolves to {home}/victim-remove.
        let victim = temp.path().join("victim-remove");
        fs::create_dir_all(&victim).expect("create victim dir");
        fs::create_dir_all(temp.path().join(".claude").join("skills")).expect("create app dir");

        let result = SkillService::remove_from_app("../../victim-remove", &AppType::Claude);

        assert!(result.is_err(), "remove_from_app must reject traversal");
        assert!(victim.exists(), "victim directory must not be deleted");
    }

    #[test]
    #[serial_test::serial]
    fn aliased_skill_roots_reject_sync_and_remove_without_deleting_the_source() {
        let _location = StorageLocationGuard::set(SkillStorageLocation::Unified);
        let temp = tempdir().expect("tempdir");
        let _home = TestHomeGuard::set(temp.path());
        let _pi_dir =
            crate::pi_config::test_support::TestAgentDir::at(&temp.path().join(".agents"));

        let source = SkillService::get_ssot_dir()
            .expect("SSOT")
            .join("test-skill");
        write_skill(&source, "managed");

        let sync = SkillService::sync_to_app_dir("test-skill", &AppType::Pi);
        assert!(sync.is_err(), "an aliased deployment root must be rejected");
        assert!(source.join("SKILL.md").exists());

        let remove = SkillService::remove_from_app("test-skill", &AppType::Pi);
        assert!(
            remove.is_err(),
            "an aliased deployment root must be rejected"
        );
        assert!(
            source.join("SKILL.md").exists(),
            "rejecting the operation must leave the SSOT untouched"
        );
    }

    #[test]
    #[serial_test::serial]
    fn migrate_storage_rejects_an_aliased_destination_before_moving_skills() {
        let _location = StorageLocationGuard::set(SkillStorageLocation::CcSwitch);
        let temp = tempdir().expect("tempdir");
        let _home = TestHomeGuard::set(temp.path());
        let _pi_dir =
            crate::pi_config::test_support::TestAgentDir::at(&temp.path().join(".agents"));

        let db = std::sync::Arc::new(Database::memory().expect("memory db"));
        let skill = poisoned_skill("owner/repo:skill", "test-skill");
        db.save_skill(&skill).expect("save skill");
        let source = SkillService::get_ssot_dir()
            .expect("SSOT")
            .join("test-skill");
        write_skill(&source, "managed");

        let migration = SkillService::migrate_storage(&db, SkillStorageLocation::Unified);

        assert!(
            migration.is_err(),
            "migration must reject a target that aliases an app deployment root"
        );
        assert_eq!(
            crate::settings::get_skill_storage_location(),
            SkillStorageLocation::CcSwitch
        );
        assert!(
            source.join("SKILL.md").exists(),
            "validation must happen before any source is moved"
        );
    }

    #[test]
    #[serial_test::serial]
    fn migrate_storage_safely_leaves_an_existing_pi_ssot_alias() {
        let _location = StorageLocationGuard::set(SkillStorageLocation::Unified);
        let temp = tempdir().expect("tempdir");
        let _home = TestHomeGuard::set(temp.path());
        let _pi_dir =
            crate::pi_config::test_support::TestAgentDir::at(&temp.path().join(".agents"));

        let db = std::sync::Arc::new(Database::memory().expect("memory db"));
        let skill = poisoned_skill("owner/repo:skill", "test-skill");
        db.save_skill(&skill).expect("save skill");
        let old_source = SkillService::get_ssot_dir()
            .expect("SSOT")
            .join("test-skill");
        write_skill(&old_source, "managed");

        let result = SkillService::migrate_storage(&db, SkillStorageLocation::CcSwitch)
            .expect("migrate away from alias");
        let new_source = crate::infrastructure::paths::product_data_dir()
            .join("skills")
            .join("test-skill");
        let pi_skill = temp
            .path()
            .join(".agents")
            .join("skills")
            .join("test-skill");

        assert_eq!(result.migrated_count, 1);
        assert!(result.errors.is_empty(), "{:?}", result.errors);
        assert!(new_source.join("SKILL.md").exists());
        assert!(
            pi_skill.join("SKILL.md").exists(),
            "the previously native Pi skill must stay active after migration"
        );
    }

    #[test]
    #[serial_test::serial]
    fn uninstall_rejects_traversal_directory_from_db_row() {
        let temp = tempdir().expect("tempdir");
        let _guard = TestHomeGuard::set(temp.path());

        // Simulate dirty data pushed in by a sync import: directory contains traversal (save_skill does
        // not validate, matching the effect of import_sql_string_for_sync). An unvalidated invalid
        // directory could escape the product SSOT and touch any sibling directory.
        let victim = temp.path().join("victim-uninstall");
        fs::create_dir_all(&victim).expect("create victim dir");

        let db = std::sync::Arc::new(Database::memory().expect("memory db"));
        let skill = poisoned_skill("owner/repo:evil", "../../victim-uninstall");
        db.save_skill(&skill).expect("seed poisoned row");

        let result = SkillService::uninstall(&db, &skill.id)
            .expect("uninstall must remove the poisoned database row");

        // The dangerous filesystem operations must be skipped...
        assert!(victim.exists(), "victim directory must not be deleted");
        // ...but the record itself must still be deletable. db.delete_skill is called only by uninstall in
        // the whole project and is not exposed as a command, so returning Err here would make the dirty row unremovable from the UI.
        assert!(
            result.pi_cleanup_incomplete,
            "skipped filesystem cleanup must be reported"
        );
        assert!(
            db.get_installed_skill(&skill.id)
                .expect("query skill")
                .is_none(),
            "poisoned row must be deleted from the database"
        );
    }

    #[cfg(unix)]
    #[test]
    #[serial_test::serial]
    fn migrate_storage_retargets_a_managed_pi_symlink() {
        let _location = StorageLocationGuard::set(SkillStorageLocation::CcSwitch);
        let temp = tempdir().expect("tempdir");
        let _home = TestHomeGuard::set(temp.path());
        let _pi_dir = crate::pi_config::test_support::TestAgentDir::new();

        let db = std::sync::Arc::new(Database::memory().expect("memory db"));
        let skill = poisoned_skill("owner/repo:skill", "test-skill");
        db.save_skill(&skill).expect("save skill");
        let old_source = SkillService::get_ssot_dir().unwrap().join("test-skill");
        write_skill(&old_source, "managed");
        SkillService::sync_to_app_dir("test-skill", &AppType::Pi).expect("enable Pi skill");
        let pi_skill = SkillService::get_app_skills_dir(&AppType::Pi)
            .unwrap()
            .join("test-skill");
        assert!(SkillService::is_symlink(&pi_skill));

        let result = SkillService::migrate_storage(&db, SkillStorageLocation::Unified)
            .expect("migrate storage");
        let new_source = temp
            .path()
            .join(".agents")
            .join("skills")
            .join("test-skill");

        assert_eq!(result.migrated_count, 1);
        assert!(result.errors.is_empty(), "{:?}", result.errors);
        assert!(!old_source.exists());
        assert!(new_source.exists());
        assert_eq!(
            pi_skill.canonicalize().expect("resolve Pi symlink"),
            new_source.canonicalize().expect("resolve new source")
        );
    }

    #[cfg(unix)]
    #[test]
    #[serial_test::serial]
    fn migrate_storage_retargets_an_equivalent_relative_pi_symlink() {
        let _location = StorageLocationGuard::set(SkillStorageLocation::CcSwitch);
        let temp = tempdir().expect("tempdir");
        let _home = TestHomeGuard::set(temp.path());
        let _pi_dir =
            crate::pi_config::test_support::TestAgentDir::at(&temp.path().join("pi-agent"));

        let db = std::sync::Arc::new(Database::memory().expect("memory db"));
        let skill = poisoned_skill("owner/repo:skill", "test-skill");
        db.save_skill(&skill).expect("save skill");
        let old_source = SkillService::get_ssot_dir().unwrap().join("test-skill");
        write_skill(&old_source, "managed");

        let pi_skill = SkillService::get_app_skills_dir(&AppType::Pi)
            .unwrap()
            .join("test-skill");
        fs::create_dir_all(pi_skill.parent().expect("Pi skills directory"))
            .expect("create Pi skills directory");
        std::os::unix::fs::symlink(Path::new("../../ai-manager/skills/test-skill"), &pi_skill)
            .expect("create relative Pi symlink");

        let result = SkillService::migrate_storage(&db, SkillStorageLocation::Unified)
            .expect("migrate storage");
        let new_source = temp
            .path()
            .join(".agents")
            .join("skills")
            .join("test-skill");

        assert_eq!(result.migrated_count, 1);
        assert!(result.errors.is_empty(), "{:?}", result.errors);
        assert_eq!(
            pi_skill.canonicalize().expect("resolve Pi symlink"),
            new_source.canonicalize().expect("resolve new source")
        );
    }

    #[test]
    #[serial_test::serial]
    fn migrate_storage_skips_bad_rows_without_moving_foreign_dirs() {
        let _location = StorageLocationGuard::set(SkillStorageLocation::CcSwitch);

        let temp = tempdir().expect("tempdir");
        let _guard = TestHomeGuard::set(temp.path());

        // migrate_storage calls fs::rename / remove_dir_all, so a dirty directory could move or delete
        // any directory outside the SSOT.
        let victim = temp.path().join("victim-migrate");
        fs::create_dir_all(&victim).expect("create victim dir");

        let db = std::sync::Arc::new(Database::memory().expect("memory db"));
        let skill = poisoned_skill("owner/repo:evil", "../../victim-migrate");
        db.save_skill(&skill).expect("seed poisoned row");

        // The target must differ from the current location, otherwise the function short-circuits at current == target
        let result = SkillService::migrate_storage(&db, SkillStorageLocation::Unified)
            .expect("migration must not abort");

        assert!(victim.exists(), "foreign directory must not be moved away");
        assert_eq!(
            result.migrated_count, 0,
            "poisoned row must not count as migrated"
        );
        assert!(
            !result.errors.is_empty(),
            "the skipped row must be reported through the errors channel"
        );
    }

    #[test]
    #[serial_test::serial]
    fn uninstall_backup_source_rejects_traversal_directory() {
        let temp = tempdir().expect("tempdir");
        let _guard = TestHomeGuard::set(temp.path());

        // The backup source is copied wholesale into skill-backups and listed in the UI -> an arbitrary file exfiltration surface.
        let secrets = temp.path().join("secrets");
        fs::create_dir_all(&secrets).expect("create secrets dir");
        fs::write(secrets.join("id_rsa"), b"PRIVATE").expect("write secret");

        let skill = poisoned_skill("owner/repo:evil", "../../secrets");
        let result = SkillService::resolve_uninstall_backup_source_excluding(&skill, None);

        assert!(
            result.is_err(),
            "backup source must reject a traversal directory"
        );
    }

    #[test]
    #[serial_test::serial]
    fn sync_to_app_skips_bad_rows_instead_of_aborting_the_whole_app() {
        let temp = tempdir().expect("tempdir");
        let _guard = TestHomeGuard::set(temp.path());

        let ssot_dir = SkillService::get_ssot_dir().expect("ssot dir");
        write_skill(&ssot_dir.join("good-skill"), "good");

        let db = std::sync::Arc::new(Database::memory().expect("memory db"));
        // One dirty row plus one normal row. The dirty row comes from a sync import / legacy data and
        // must not drag the normal one down - sync_to_app runs during a provider switch, and aborting everything would break every skill.
        //
        // The names deliberately put the dirty row first: the query is `ORDER BY name ASC`, so it must be
        // processed first, otherwise the premise "it would abort when unfixed" does not hold and the test proves nothing.
        let mut bad = poisoned_skill("owner/repo:bad", "../../escape-sync");
        bad.name = "a-poisoned".to_string();
        bad.apps = SkillApps::only(&AppType::Claude);
        db.save_skill(&bad).expect("seed poisoned row");

        let mut good = poisoned_skill("owner/repo:good", "good-skill");
        good.name = "z-healthy".to_string();
        good.apps = SkillApps::only(&AppType::Claude);
        db.save_skill(&good).expect("seed good row");

        SkillService::sync_to_app(&db, &AppType::Claude).expect("sync must not abort");

        let app_dir = SkillService::get_app_skills_dir(&AppType::Claude).expect("app dir");
        assert!(
            app_dir.join("good-skill").exists(),
            "the healthy skill must still be synced despite the poisoned row"
        );
    }

    #[test]
    // serial: mutually exclusive with the backup/s3_sync/deeplink tests that also read and write the
    // process-wide AI_MANAGER_TEST_HOME; EnvGuard only restores, it does not provide exclusion.
    #[serial_test::serial]
    fn get_app_skills_dir_honors_test_home_override() {
        // Regression: dirs::home_dir() used to be called directly, bypassing AI_MANAGER_TEST_HOME - on
        // Unix it happened to match $HOME so the test passed, while on Windows dirs goes through the
        // Known Folder API and the isolation broke entirely (tests/skill_sync.rs scanned the runner's real home).
        struct EnvGuard(Option<std::ffi::OsString>);
        impl Drop for EnvGuard {
            fn drop(&mut self) {
                match self.0.take() {
                    Some(value) => std::env::set_var("AI_MANAGER_TEST_HOME", value),
                    None => std::env::remove_var("AI_MANAGER_TEST_HOME"),
                }
            }
        }
        let temp = tempdir().expect("tempdir");
        let _guard = EnvGuard(std::env::var_os("AI_MANAGER_TEST_HOME"));
        std::env::set_var("AI_MANAGER_TEST_HOME", temp.path());

        let dir =
            SkillService::get_app_skills_dir(&AppType::Claude).expect("resolve claude skills dir");
        assert!(
            dir.starts_with(temp.path()),
            "skills dir must live under the overridden test home, got {}",
            dir.display()
        );
    }

    #[test]
    fn resolve_skill_source_dir_returns_repo_root_for_root_level_skill() {
        let temp = tempdir().expect("tempdir");
        write_skill(temp.path(), "Root Skill");

        let resolved = SkillService::resolve_skill_source_dir(temp.path(), "last30days-skill-cn")
            .expect("root-level skill should resolve to the extracted repo root");

        assert_eq!(resolved, temp.path());
    }

    #[test]
    fn resolve_skill_source_dir_returns_direct_nested_directory_when_present() {
        let temp = tempdir().expect("tempdir");
        let nested = temp.path().join("skills").join("nested-skill");
        write_skill(&nested, "Nested Skill");

        let resolved = SkillService::resolve_skill_source_dir(temp.path(), "skills/nested-skill")
            .expect("nested skill should resolve from its relative source path");

        assert_eq!(resolved, nested);
    }

    #[test]
    fn resolve_skill_source_dir_falls_back_to_matching_install_name() {
        let temp = tempdir().expect("tempdir");
        let nested = temp.path().join("skills").join("nested-skill");
        write_skill(&nested, "Nested Skill");

        let resolved = SkillService::resolve_skill_source_dir(temp.path(), "nested-skill")
            .expect("install name should fall back to the matching discovered skill directory");

        assert_eq!(resolved, nested);
    }

    #[test]
    fn replace_dest_with_copy_rejects_empty_source_without_touching_existing_dest() {
        let temp = tempdir().expect("tempdir");
        let source = temp.path().join("source-skill");
        let dest = temp.path().join("app-skills").join("source-skill");
        fs::create_dir_all(&source).expect("create empty source");
        write_skill(&dest, "Existing Skill");

        let err = SkillService::replace_dest_with_copy(&source, &dest, "source-skill", true)
            .expect_err("empty source should not replace existing app skill");

        assert!(
            err.to_string().contains("SKILL.md"),
            "unexpected error: {err:#}"
        );
        assert!(
            dest.join("SKILL.md").is_file(),
            "existing destination skill should be preserved"
        );
    }

    #[test]
    fn resolve_skill_source_dir_rejects_same_name_wrapper_without_skill_md() {
        // Reproduces issue #4141: the ast-grep/agent-skill layout. The repository root has a same-named
        // directory ast-grep/ (a plugin package with no SKILL.md) while the real skill is at ast-grep/skills/ast-grep/SKILL.md.
        let temp = tempdir().expect("tempdir");
        let wrapper = temp.path().join("ast-grep");
        fs::create_dir_all(wrapper.join(".claude-plugin")).expect("create wrapper plugin dir");
        fs::write(
            wrapper.join(".claude-plugin").join("plugin.json"),
            "{\"name\":\"ast-grep\"}",
        )
        .expect("write plugin.json");
        let real_skill = wrapper.join("skills").join("ast-grep");
        write_skill(&real_skill, "ast-grep");

        // directory only carries the skill name "ast-grep" (the skills.sh API semantics) and must not hit the empty wrapper.
        let resolved = SkillService::resolve_skill_source_dir(temp.path(), "ast-grep")
            .expect("should resolve to the inner skill dir, not the same-name wrapper");

        assert_eq!(resolved, real_skill);
        assert!(resolved.join("SKILL.md").is_file());
    }

    #[test]
    fn resolve_skill_source_dir_finds_two_level_catalog_skill() {
        // catalog layout: skills/category/foo/SKILL.md (depth 3, reachable by find_skill_dir_by_name).
        let temp = tempdir().expect("tempdir");
        let catalog_skill = temp.path().join("skills").join("category").join("foo");
        write_skill(&catalog_skill, "Foo Skill");

        let resolved = SkillService::resolve_skill_source_dir(temp.path(), "foo")
            .expect("should resolve the two-level catalog skill by name");

        assert_eq!(resolved, catalog_skill);
    }

    #[test]
    fn resolve_skill_source_dir_returns_none_for_wrapper_without_inner_skill() {
        // When a same-named wrapper exists without SKILL.md and there is no inner skill / root SKILL.md
        // to fall back on, None must be returned - guarding the negative case of the #4141 bug class (an empty shell directory is not a source directory).
        let temp = tempdir().expect("tempdir");
        let wrapper = temp.path().join("ast-grep");
        fs::create_dir_all(wrapper.join(".claude-plugin")).expect("create wrapper plugin dir");
        fs::write(
            wrapper.join(".claude-plugin").join("plugin.json"),
            "{\"name\":\"ast-grep\"}",
        )
        .expect("write plugin.json");

        let resolved = SkillService::resolve_skill_source_dir(temp.path(), "ast-grep");
        assert!(
            resolved.is_none(),
            "wrapper dir without SKILL.md and no inner skill must resolve to None, got {:?}",
            resolved
        );
    }

    #[test]
    fn resolve_skill_source_dir_returns_none_when_no_skill_md_anywhere() {
        let temp = tempdir().expect("tempdir");
        fs::create_dir_all(temp.path().join("skills").join("foo")).expect("create empty skill dir");
        fs::write(temp.path().join("README.md"), "no skills here").expect("write README");

        let resolved = SkillService::resolve_skill_source_dir(temp.path(), "foo");
        assert!(
            resolved.is_none(),
            "no SKILL.md anywhere must resolve to None"
        );
    }

    #[test]
    fn choose_doc_path_prefers_resolved_source_over_stale_readme_url() {
        // Reconstruction of #6111: skills.sh installs a nested-directory skill - directory is the skillId
        // (last segment), readme_url is the repository root URL, and the real nested path can only come
        // from the resolved source directory. Falling back to "readme_url extraction first, directory join
        // second" makes this assertion fail (the old logic produced alibabacloud-cli-guidance/SKILL.md -> 404).
        let doc_path = SkillService::choose_doc_path(
            Some("skills/developertools/solutions/alibabacloud-cli-guidance/SKILL.md".to_string()),
            Some("https://github.com/aliyun/alibabacloud-aiops-skills"),
            "alibabacloud-cli-guidance",
        );
        assert_eq!(
            doc_path,
            "skills/developertools/solutions/alibabacloud-cli-guidance/SKILL.md"
        );
    }

    #[test]
    fn choose_doc_path_falls_back_to_readme_url_path_then_directory() {
        // With no resolution result (the SSOT directory already exists and nothing was re-downloaded), the
        // in-repository path of the legacy readme_url is reused, handling both blob/tree forms and appending SKILL.md
        let doc_path = SkillService::choose_doc_path(
            None,
            Some("https://github.com/o/r/tree/main/skills/foo"),
            "foo",
        );
        assert_eq!(doc_path, "skills/foo/SKILL.md");

        // A legacy readme_url that is already a full doc path is kept as is
        let doc_path = SkillService::choose_doc_path(
            None,
            Some("https://github.com/o/r/blob/main/skills/foo/SKILL.md"),
            "foo",
        );
        assert_eq!(doc_path, "skills/foo/SKILL.md");

        // With neither available, the path is joined from directory
        let doc_path = SkillService::choose_doc_path(None, None, "skills/foo");
        assert_eq!(doc_path, "skills/foo/SKILL.md");
    }

    #[test]
    fn doc_path_for_source_returns_repo_relative_skill_md_path() {
        let temp = tempdir().expect("tempdir");
        let nested = temp
            .path()
            .join("skills")
            .join("developertools")
            .join("solutions")
            .join("foo");
        fs::create_dir_all(&nested).expect("create nested dirs");

        assert_eq!(
            SkillService::doc_path_for_source(temp.path(), &nested),
            Some("skills/developertools/solutions/foo/SKILL.md".to_string())
        );
        // The source directory is the repository root: the doc path is simply SKILL.md at the root
        assert_eq!(
            SkillService::doc_path_for_source(temp.path(), temp.path()),
            Some("SKILL.md".to_string())
        );
        // Outside the repository root: None (the caller already checks containment, this is a defensive fallback)
        assert_eq!(
            SkillService::doc_path_for_source(temp.path(), std::path::Path::new("/elsewhere")),
            None
        );
    }

    /// serial: it changes the process-wide AI_MANAGER_TEST_HOME.
    #[test]
    #[serial_test::serial]
    fn copy_detected_to_app_dir_makes_a_real_independent_folder() {
        let temp = tempdir().expect("tempdir");
        let _home = TestHomeGuard::set(temp.path());
        let source = temp.path().join(".claude").join("skills").join("unity-cli");
        write_skill(&source, "Unity CLI");

        SkillService::copy_detected_to_app_dir(&source, "unity-cli", &AppType::Codex)
            .expect("copy the detected Skill into Codex");

        let dest = temp.path().join(".codex").join("skills").join("unity-cli");
        assert!(
            !fs::symlink_metadata(&dest)
                .expect("read the copy metadata")
                .file_type()
                .is_symlink(),
            "the copy must not be a link back to a directory we do not own"
        );
        assert_eq!(
            fs::read(dest.join("SKILL.md")).expect("the copy has the document"),
            fs::read(source.join("SKILL.md")).expect("the source is untouched")
        );
    }

    #[test]
    #[serial_test::serial]
    fn copy_detected_to_app_dir_refuses_to_overwrite_whatever_is_already_there() {
        let temp = tempdir().expect("tempdir");
        let _home = TestHomeGuard::set(temp.path());
        let source = temp.path().join(".claude").join("skills").join("unity-cli");
        write_skill(&source, "Unity CLI");
        let occupied = temp.path().join(".codex").join("skills").join("unity-cli");
        write_skill(&occupied, "Their own Unity CLI");
        let theirs = fs::read(occupied.join("SKILL.md")).expect("read their document");

        let err = SkillService::copy_detected_to_app_dir(&source, "unity-cli", &AppType::Codex)
            .expect_err("an occupied destination is never overwritten");

        assert!(err.to_string().contains("unity-cli"), "unexpected: {err:#}");
        assert_eq!(
            fs::read(occupied.join("SKILL.md")).expect("their document survived"),
            theirs
        );
    }

    #[test]
    #[serial_test::serial]
    fn copy_detected_to_app_dir_is_a_no_op_when_the_source_is_the_destination() {
        let temp = tempdir().expect("tempdir");
        let _home = TestHomeGuard::set(temp.path());
        let source = temp.path().join(".codex").join("skills").join("unity-cli");
        write_skill(&source, "Unity CLI");

        SkillService::copy_detected_to_app_dir(&source, "unity-cli", &AppType::Codex)
            .expect("copying a Skill onto itself has nothing to do");

        assert!(source.join("SKILL.md").is_file());
    }

    #[test]
    #[serial_test::serial]
    #[cfg(unix)]
    fn copy_detected_to_app_dir_refuses_a_source_that_contains_a_symlink() {
        let temp = tempdir().expect("tempdir");
        let _home = TestHomeGuard::set(temp.path());
        let outside = temp.path().join("outside.txt");
        fs::write(&outside, b"private").expect("write the outside file");
        let source = temp.path().join(".claude").join("skills").join("unity-cli");
        write_skill(&source, "Unity CLI");
        std::os::unix::fs::symlink(&outside, source.join("notes.txt")).expect("plant the link");

        let err = SkillService::copy_detected_to_app_dir(&source, "unity-cli", &AppType::Codex)
            .expect_err("a third-party source with a link is not copied");

        assert!(
            err.to_string().contains("symbolic link"),
            "unexpected error: {err:#}"
        );
        // The temporary staging directory is cleaned up, and nothing lands.
        assert!(!temp
            .path()
            .join(".codex")
            .join("skills")
            .join("unity-cli")
            .exists());
        let leftovers = fs::read_dir(temp.path().join(".codex").join("skills"))
            .map(|entries| entries.count())
            .unwrap_or(0);
        assert_eq!(
            leftovers, 0,
            "a half-written temporary directory was left behind"
        );
    }

    #[test]
    #[serial_test::serial]
    fn copy_detected_to_app_dir_rejects_a_directory_name_that_could_traverse() {
        let temp = tempdir().expect("tempdir");
        let _home = TestHomeGuard::set(temp.path());
        let source = temp.path().join(".claude").join("skills").join("unity-cli");
        write_skill(&source, "Unity CLI");

        let err = SkillService::copy_detected_to_app_dir(&source, "../escape", &AppType::Codex)
            .expect_err("a traversing directory name is refused");

        assert!(
            err.to_string().contains("path traversal"),
            "unexpected error: {err:#}"
        );
    }
}
