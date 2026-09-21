use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::str::FromStr;

use crate::services::skill::SkillStore;

/// MCP server app state (marks which clients the server is applied to)
#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq)]
pub struct McpApps {
    #[serde(default)]
    pub claude: bool,
    #[serde(default)]
    pub codex: bool,
    #[serde(default)]
    pub gemini: bool,
    #[serde(default)]
    pub grokbuild: bool,
    #[serde(default)]
    pub opencode: bool,
    #[serde(default)]
    pub hermes: bool,
    #[serde(default)]
    pub claude_desktop: bool,
}

impl McpApps {
    /// Check whether the given app is enabled
    pub fn is_enabled_for(&self, app: &AppType) -> bool {
        match app {
            AppType::Claude => self.claude,
            AppType::Codex => self.codex,
            AppType::Gemini => self.gemini,
            AppType::GrokBuild => self.grokbuild,
            AppType::OpenCode => self.opencode,
            AppType::OpenClaw => false, // OpenClaw doesn't support MCP
            AppType::Hermes => self.hermes,
            AppType::Pi => false, // Pi core has no native MCP registry.
            AppType::ClaudeDesktop => self.claude_desktop,
        }
    }

    /// Set the enabled state for the given app
    pub fn set_enabled_for(&mut self, app: &AppType, enabled: bool) {
        match app {
            AppType::Claude => self.claude = enabled,
            AppType::Codex => self.codex = enabled,
            AppType::Gemini => self.gemini = enabled,
            AppType::GrokBuild => self.grokbuild = enabled,
            AppType::OpenCode => self.opencode = enabled,
            AppType::OpenClaw => {} // OpenClaw doesn't support MCP, ignore
            AppType::Hermes => self.hermes = enabled,
            AppType::Pi => {} // Pi core has no native MCP registry.
            AppType::ClaudeDesktop => self.claude_desktop = enabled,
        }
    }

    /// Return the list of all enabled apps
    pub fn enabled_apps(&self) -> Vec<AppType> {
        let mut apps = Vec::new();
        if self.claude {
            apps.push(AppType::Claude);
        }
        if self.codex {
            apps.push(AppType::Codex);
        }
        if self.gemini {
            apps.push(AppType::Gemini);
        }
        if self.grokbuild {
            apps.push(AppType::GrokBuild);
        }
        if self.opencode {
            apps.push(AppType::OpenCode);
        }
        if self.hermes {
            apps.push(AppType::Hermes);
        }
        if self.claude_desktop {
            apps.push(AppType::ClaudeDesktop);
        }
        apps
    }

    /// Check whether no app is enabled
    pub fn is_empty(&self) -> bool {
        !self.claude
            && !self.codex
            && !self.gemini
            && !self.grokbuild
            && !self.opencode
            && !self.hermes
            && !self.claude_desktop
    }
}

/// Skill app enablement state (marks which clients the skill is applied to)
#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq)]
pub struct SkillApps {
    #[serde(default)]
    pub claude: bool,
    #[serde(default)]
    pub codex: bool,
    #[serde(default)]
    pub gemini: bool,
    #[serde(default)]
    pub grokbuild: bool,
    #[serde(default)]
    pub opencode: bool,
    #[serde(default)]
    pub hermes: bool,
    #[serde(default)]
    pub pi: bool,
}

impl SkillApps {
    /// Check whether the given app is enabled
    pub fn is_enabled_for(&self, app: &AppType) -> bool {
        match app {
            AppType::Claude => self.claude,
            AppType::Codex => self.codex,
            AppType::Gemini => self.gemini,
            AppType::GrokBuild => self.grokbuild,
            AppType::OpenCode => self.opencode,
            AppType::Hermes => self.hermes,
            AppType::Pi => self.pi,
            AppType::OpenClaw => false, // OpenClaw doesn't support Skills
            AppType::ClaudeDesktop => false,
        }
    }

    /// Set the enabled state for the given app
    pub fn set_enabled_for(&mut self, app: &AppType, enabled: bool) {
        match app {
            AppType::Claude => self.claude = enabled,
            AppType::Codex => self.codex = enabled,
            AppType::Gemini => self.gemini = enabled,
            AppType::GrokBuild => self.grokbuild = enabled,
            AppType::OpenCode => self.opencode = enabled,
            AppType::Hermes => self.hermes = enabled,
            AppType::Pi => self.pi = enabled,
            AppType::OpenClaw => {} // OpenClaw doesn't support Skills, ignore
            AppType::ClaudeDesktop => {} // Claude Desktop 3P profiles don't use CC Switch skill sync
        }
    }

    /// Return the list of all enabled apps
    pub fn enabled_apps(&self) -> Vec<AppType> {
        let mut apps = Vec::new();
        if self.claude {
            apps.push(AppType::Claude);
        }
        if self.codex {
            apps.push(AppType::Codex);
        }
        if self.gemini {
            apps.push(AppType::Gemini);
        }
        if self.grokbuild {
            apps.push(AppType::GrokBuild);
        }
        if self.opencode {
            apps.push(AppType::OpenCode);
        }
        if self.hermes {
            apps.push(AppType::Hermes);
        }
        if self.pi {
            apps.push(AppType::Pi);
        }
        apps
    }

    /// Check whether no app is enabled
    pub fn is_empty(&self) -> bool {
        !self.claude
            && !self.codex
            && !self.gemini
            && !self.grokbuild
            && !self.opencode
            && !self.hermes
            && !self.pi
    }

    /// Enable only the given app (all others are disabled)
    pub fn only(app: &AppType) -> Self {
        let mut apps = Self::default();
        apps.set_enabled_for(app, true);
        apps
    }

    /// Build the enablement state from a list of source tags
    ///
    /// A tag matching AppType::as_str() enables the corresponding app;
    /// other tags (such as "agents" or "cc-switch") are ignored.
    pub fn from_labels(labels: &[String]) -> Self {
        let mut apps = Self::default();
        for label in labels {
            if let Ok(app) = label.parse::<AppType>() {
                apps.set_enabled_for(&app, true);
            }
        }
        apps
    }
}

/// An installed skill (unified structure since v3.10.0)
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct InstalledSkill {
    /// Unique identifier (format: "owner/repo:directory" or "local:directory")
    pub id: String,
    /// Display name
    pub name: String,
    /// Description
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    /// Install directory name (subdirectory name inside the SSOT directory)
    pub directory: String,
    /// Repository owner (GitHub user/organization)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub repo_owner: Option<String>,
    /// Repository name
    #[serde(skip_serializing_if = "Option::is_none")]
    pub repo_name: Option<String>,
    /// Repository branch
    #[serde(skip_serializing_if = "Option::is_none")]
    pub repo_branch: Option<String>,
    /// README URL
    #[serde(skip_serializing_if = "Option::is_none")]
    pub readme_url: Option<String>,
    /// Per-app enablement state
    pub apps: SkillApps,
    /// Install time (Unix timestamp)
    pub installed_at: i64,
    /// Content hash (SHA-256, used for update detection)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub content_hash: Option<String>,
    /// Last update time (Unix timestamp, 0 = never updated)
    #[serde(default)]
    pub updated_at: i64,
}

/// An unmanaged skill (found in an app directory but not managed by CC Switch)
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UnmanagedSkill {
    /// Directory name
    pub directory: String,
    /// Display name (parsed from SKILL.md)
    pub name: String,
    /// Description
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    /// App directories it was found in (e.g. ["claude", "codex"])
    pub found_in: Vec<String>,
    /// Discovery path (the first matching full path)
    pub path: String,
}

/// MCP server definition (unified structure since v3.7.0)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct McpServer {
    pub id: String,
    pub name: String,
    pub server: serde_json::Value,
    pub apps: McpApps,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub homepage: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub docs: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub tags: Vec<String>,
}

/// MCP config, per-client (v3.6.x and earlier, kept for backward compatibility)
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct McpConfig {
    /// Server definitions keyed by id (a loose JSON object including UI helper fields such as enabled/source)
    #[serde(default)]
    pub servers: HashMap<String, serde_json::Value>,
}

impl McpConfig {
    /// Check whether the config is empty
    pub fn is_empty(&self) -> bool {
        self.servers.is_empty()
    }
}

/// MCP root config (v3.7.0 keeps both the new and the old structure)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct McpRoot {
    /// Unified MCP server storage (v3.7.0+)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub servers: Option<HashMap<String, McpServer>>,

    /// Legacy per-app storage (v3.6.x and earlier, kept for migration)
    #[serde(default, skip_serializing_if = "McpConfig::is_empty")]
    pub claude: McpConfig,
    #[serde(
        rename = "claude-desktop",
        alias = "claudeDesktop",
        alias = "claude_desktop",
        default,
        skip_serializing_if = "McpConfig::is_empty"
    )]
    pub claude_desktop: McpConfig,
    #[serde(default, skip_serializing_if = "McpConfig::is_empty")]
    pub codex: McpConfig,
    #[serde(default, skip_serializing_if = "McpConfig::is_empty")]
    pub gemini: McpConfig,
    #[serde(default, skip_serializing_if = "McpConfig::is_empty")]
    pub grokbuild: McpConfig,
    /// OpenCode MCP config (v4.0.0+, actually uses opencode.json)
    #[serde(default, skip_serializing_if = "McpConfig::is_empty")]
    pub opencode: McpConfig,
    /// OpenClaw MCP config (v4.1.0+, actually uses openclaw.json)
    #[serde(default, skip_serializing_if = "McpConfig::is_empty")]
    pub openclaw: McpConfig,
    /// Hermes MCP config (actually uses config.yaml)
    #[serde(default, skip_serializing_if = "McpConfig::is_empty")]
    pub hermes: McpConfig,
}

impl Default for McpRoot {
    fn default() -> Self {
        Self {
            // v3.7.0+ defaults to the new unified structure (an empty HashMap)
            servers: Some(HashMap::new()),
            // The legacy structure stays empty; it only exists to migrate deserialized old configs
            claude: McpConfig::default(),
            claude_desktop: McpConfig::default(),
            codex: McpConfig::default(),
            gemini: McpConfig::default(),
            grokbuild: McpConfig::default(),
            opencode: McpConfig::default(),
            openclaw: McpConfig::default(),
            hermes: McpConfig::default(),
        }
    }
}

/// Prompt config, per-client
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct PromptConfig {
    #[serde(default)]
    pub prompts: HashMap<String, crate::prompt::Prompt>,
}

/// Prompt root: maintained separately per client
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct PromptRoot {
    #[serde(default)]
    pub claude: PromptConfig,
    #[serde(
        rename = "claude-desktop",
        alias = "claudeDesktop",
        alias = "claude_desktop",
        default
    )]
    pub claude_desktop: PromptConfig,
    #[serde(default)]
    pub codex: PromptConfig,
    #[serde(default)]
    pub gemini: PromptConfig,
    #[serde(default)]
    pub grokbuild: PromptConfig,
    #[serde(default)]
    pub opencode: PromptConfig,
    #[serde(default)]
    pub openclaw: PromptConfig,
    #[serde(default)]
    pub hermes: PromptConfig,
}

use crate::config::{copy_file, get_app_config_path, write_json_file};
use crate::error::AppError;
use crate::prompt_files::prompt_file_path;
use crate::provider::ProviderManager;

/// App type
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum AppType {
    Claude,
    #[serde(
        rename = "claude-desktop",
        alias = "claude_desktop",
        alias = "claudeDesktop"
    )]
    ClaudeDesktop,
    Codex,
    Gemini,
    GrokBuild,
    OpenCode,
    OpenClaw,
    Hermes,
    Pi,
}

impl AppType {
    pub fn as_str(&self) -> &str {
        match self {
            AppType::Claude => "claude",
            AppType::ClaudeDesktop => "claude-desktop",
            AppType::Codex => "codex",
            AppType::Gemini => "gemini",
            AppType::GrokBuild => "grokbuild",
            AppType::OpenCode => "opencode",
            AppType::OpenClaw => "openclaw",
            AppType::Hermes => "hermes",
            AppType::Pi => "pi",
        }
    }

    /// Check if this app uses additive mode
    ///
    /// - Switch mode (false): Only the current provider is written to live config (Claude, Codex, Gemini)
    /// - Additive mode (true): Providers coexist in native config and can be enabled independently
    ///   (OpenCode, OpenClaw, Hermes, Pi)
    pub fn is_additive_mode(&self) -> bool {
        matches!(
            self,
            AppType::OpenCode | AppType::OpenClaw | AppType::Hermes | AppType::Pi
        )
    }

    pub fn supports_local_proxy(&self) -> bool {
        matches!(
            self,
            AppType::Claude | AppType::Codex | AppType::Gemini | AppType::GrokBuild
        )
    }

    /// Return an iterator over all app types
    pub fn all() -> impl Iterator<Item = AppType> {
        [
            AppType::Claude,
            AppType::ClaudeDesktop,
            AppType::Codex,
            AppType::Gemini,
            AppType::GrokBuild,
            AppType::OpenCode,
            AppType::OpenClaw,
            AppType::Hermes,
            AppType::Pi,
        ]
        .into_iter()
    }
}

impl FromStr for AppType {
    type Err = AppError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let normalized = s.trim().to_lowercase();
        match normalized.as_str() {
            "claude" => Ok(AppType::Claude),
            "claude-desktop" | "claude_desktop" | "claudedesktop" => Ok(AppType::ClaudeDesktop),
            "codex" => Ok(AppType::Codex),
            "gemini" => Ok(AppType::Gemini),
            "grokbuild" | "grok-build" | "grok_build" | "grok" => Ok(AppType::GrokBuild),
            "opencode" => Ok(AppType::OpenCode),
            "openclaw" => Ok(AppType::OpenClaw),
            "hermes" => Ok(AppType::Hermes),
            "pi" => Ok(AppType::Pi),
            other => Err(AppError::localized(
                "unsupported_app",
                format!("Unsupported app id: '{other}'. Allowed: claude, claude-desktop, codex, gemini, grokbuild, opencode, openclaw, hermes, pi."),
            )),
        }
    }
}

/// Common config snippets (partitioned per app)
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct CommonConfigSnippets {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub claude: Option<String>,

    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub codex: Option<String>,

    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub gemini: Option<String>,

    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub opencode: Option<String>,

    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub openclaw: Option<String>,

    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub hermes: Option<String>,
}

impl CommonConfigSnippets {
    /// Return the common config snippet for the given app
    pub fn get(&self, app: &AppType) -> Option<&String> {
        match app {
            AppType::Claude => self.claude.as_ref(),
            AppType::ClaudeDesktop => None,
            AppType::Codex => self.codex.as_ref(),
            AppType::Gemini => self.gemini.as_ref(),
            AppType::GrokBuild => None,
            AppType::OpenCode => self.opencode.as_ref(),
            AppType::OpenClaw => self.openclaw.as_ref(),
            AppType::Hermes => self.hermes.as_ref(),
            AppType::Pi => None,
        }
    }

    /// Set the common config snippet for the given app
    pub fn set(&mut self, app: &AppType, snippet: Option<String>) {
        match app {
            AppType::Claude => self.claude = snippet,
            AppType::ClaudeDesktop => {}
            AppType::Codex => self.codex = snippet,
            AppType::Gemini => self.gemini = snippet,
            AppType::GrokBuild => {}
            AppType::OpenCode => self.opencode = snippet,
            AppType::OpenClaw => self.openclaw = snippet,
            AppType::Hermes => self.hermes = snippet,
            AppType::Pi => {}
        }
    }
}

/// Multi-app config structure (backward compatible)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MultiAppConfig {
    #[serde(default = "default_version")]
    pub version: u32,
    /// App managers (claude/codex)
    #[serde(flatten)]
    pub apps: HashMap<String, ProviderManager>,
    /// MCP config (partitioned per client)
    #[serde(default)]
    pub mcp: McpRoot,
    /// Prompt config (partitioned per client)
    #[serde(default)]
    pub prompts: PromptRoot,
    /// Claude skills config
    #[serde(default)]
    pub skills: SkillStore,
    /// Common config snippets (partitioned per app)
    #[serde(default)]
    pub common_config_snippets: CommonConfigSnippets,
    /// Claude common config snippet (legacy field, kept for backward-compatible migration)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub claude_common_config_snippet: Option<String>,
}

fn default_version() -> u32 {
    2
}

impl Default for MultiAppConfig {
    fn default() -> Self {
        let mut apps = HashMap::new();
        apps.insert("claude".to_string(), ProviderManager::default());
        apps.insert("claude-desktop".to_string(), ProviderManager::default());
        apps.insert("codex".to_string(), ProviderManager::default());
        apps.insert("gemini".to_string(), ProviderManager::default());
        apps.insert("grokbuild".to_string(), ProviderManager::default());
        apps.insert("opencode".to_string(), ProviderManager::default());
        apps.insert("openclaw".to_string(), ProviderManager::default());
        apps.insert("hermes".to_string(), ProviderManager::default());

        Self {
            version: 2,
            apps,
            mcp: McpRoot::default(),
            prompts: PromptRoot::default(),
            skills: SkillStore::default(),
            common_config_snippets: CommonConfigSnippets::default(),
            claude_common_config_snippet: None,
        }
    }
}

impl MultiAppConfig {
    /// Load the config from file (v2 structure only)
    pub fn load() -> Result<Self, AppError> {
        let config_path = get_app_config_path();

        if !config_path.exists() {
            log::info!("Config file does not exist; creating a new multi-app config and auto-importing prompts");
            // Use the new method, which supports auto-importing prompts
            let config = Self::default_with_auto_import()?;
            // Persist to disk immediately
            config.save()?;
            return Ok(config);
        }

        // Try to read the file
        let content =
            std::fs::read_to_string(&config_path).map_err(|e| AppError::io(&config_path, e))?;

        // Parse into a Value first so v1 can be detected strictly:
        // a top level containing both providers(object) + current(string) and none of the version/apps/mcp keys counts as v1
        let value: serde_json::Value =
            serde_json::from_str(&content).map_err(|e| AppError::json(&config_path, e))?;
        let is_v1 = value.as_object().is_some_and(|map| {
            let has_providers = map.get("providers").map(|v| v.is_object()).unwrap_or(false);
            let has_current = map.get("current").map(|v| v.is_string()).unwrap_or(false);
            // Necessary and sufficient condition for v1: providers and current exist while apps does not (version/mcp may exist but do not prove v2)
            let has_apps = map.contains_key("apps");
            has_providers && has_current && !has_apps
        });
        if is_v1 {
            let config_path_display = config_path.display();
            return Err(AppError::localized(
                "config.unsupported_v1",
                format!("Detected a legacy v1 config format. Runtime auto-migration is no longer supported in this version.\n\nSolutions:\n1. Install v3.2.x for a one-time auto-migration\n2. Or edit {config_path_display} by hand and adjust the top-level structure to:\n   {{\"version\": 2, \"claude\": {{...}}, \"codex\": {{...}}, \"mcp\": {{...}}}}\n\n"),
            ));
        }

        let has_skills_in_config = value
            .as_object()
            .is_some_and(|map| map.contains_key("skills"));

        // Parse the v2 structure
        let mut config: Self =
            serde_json::from_value(value).map_err(|e| AppError::json(&config_path, e))?;
        let mut updated = false;

        if !has_skills_in_config {
            let skills_path = crate::infrastructure::paths::product_data_dir().join("skills.json");
            if skills_path.exists() {
                match std::fs::read_to_string(&skills_path) {
                    Ok(content) => match serde_json::from_str::<SkillStore>(&content) {
                        Ok(store) => {
                            config.skills = store;
                            updated = true;
                            log::info!(
                                "Imported the Claude skills config from the legacy skills.json"
                            );
                        }
                        Err(e) => {
                            log::warn!("Failed to parse the legacy skills.json: {e}");
                        }
                    },
                    Err(e) => {
                        log::warn!("Failed to read the legacy skills.json: {e}");
                    }
                }
            }
        }

        // Make sure the gemini app exists (compatibility with older config files)
        if !config.apps.contains_key("gemini") {
            config
                .apps
                .insert("gemini".to_string(), ProviderManager::default());
            updated = true;
        }

        // Run the MCP migration (v3.6.x -> v3.7.0)
        let migrated = config.migrate_mcp_to_unified()?;
        if migrated {
            log::info!("MCP config migrated to the v3.7.0 unified structure; saving the config...");
            updated = true;
        }

        // For an existing config file written by a version that did not have the prompt
        // feature yet, and whose prompts are still empty, try to auto-import existing prompt files.
        let imported_prompts = config.maybe_auto_import_prompts_for_existing_config()?;
        if imported_prompts {
            updated = true;
        }

        // Migrate the common config snippet: claude_common_config_snippet -> common_config_snippets.claude
        if let Some(old_claude_snippet) = config.claude_common_config_snippet.take() {
            log::info!(
                "Migrating the common config: claude_common_config_snippet -> common_config_snippets.claude"
            );
            config.common_config_snippets.claude = Some(old_claude_snippet);
            updated = true;
        }

        if updated {
            log::info!("Config structure updated (MCP migration or prompt auto-import); saving the config...");
            config.save()?;
        }

        Ok(config)
    }

    /// Save the config to file
    pub fn save(&self) -> Result<(), AppError> {
        let config_path = get_app_config_path();
        // Back up to config.json.bak inside the current product config directory, then write the new content.
        if config_path.exists() {
            let backup_path = config_path.with_file_name("config.json.bak");
            if let Err(e) = copy_file(&config_path, &backup_path) {
                log::warn!("Failed to back up config.json to .bak: {e}");
            }
        }

        write_json_file(&config_path, self)?;
        Ok(())
    }

    /// Return the manager for the given app
    pub fn get_manager(&self, app: &AppType) -> Option<&ProviderManager> {
        self.apps.get(app.as_str())
    }

    /// Return the manager for the given app (mutable reference)
    pub fn get_manager_mut(&mut self, app: &AppType) -> Option<&mut ProviderManager> {
        self.apps.get_mut(app.as_str())
    }

    /// Ensure the app exists
    pub fn ensure_app(&mut self, app: &AppType) {
        if !self.apps.contains_key(app.as_str()) {
            self.apps
                .insert(app.as_str().to_string(), ProviderManager::default());
        }
    }

    /// Create the default config and auto-import existing prompt files
    fn default_with_auto_import() -> Result<Self, AppError> {
        log::info!("First launch: creating the default config and detecting prompt files");

        let mut config = Self::default();

        // Try to auto-import prompts for every app
        Self::auto_import_prompt_if_exists(&mut config, AppType::Claude)?;
        Self::auto_import_prompt_if_exists(&mut config, AppType::Codex)?;
        Self::auto_import_prompt_if_exists(&mut config, AppType::Gemini)?;
        Self::auto_import_prompt_if_exists(&mut config, AppType::GrokBuild)?;
        Self::auto_import_prompt_if_exists(&mut config, AppType::OpenCode)?;
        Self::auto_import_prompt_if_exists(&mut config, AppType::OpenClaw)?;
        Self::auto_import_prompt_if_exists(&mut config, AppType::Hermes)?;

        Ok(config)
    }

    /// Prompt auto-import logic for an already existing config file
    ///
    /// Covers the upgrade case where an older version already created config.json before
    /// the prompt feature existed. Rules:
    /// - only attempt the import when every app's prompts are empty (so users already
    ///   using the prompt feature are not disturbed)
    /// - import at most once per app, from its own prompt file (CLAUDE.md/AGENTS.md/GEMINI.md)
    ///
    /// Return value:
    /// - Ok(true)  at least one app imported its prompt successfully
    /// - Ok(false) nothing needed to be imported, or nothing was imported
    fn maybe_auto_import_prompts_for_existing_config(&mut self) -> Result<bool, AppError> {
        // If any app already has prompts configured the user is using the prompt feature, so skip the auto-import
        if !self.prompts.claude.prompts.is_empty()
            || !self.prompts.claude_desktop.prompts.is_empty()
            || !self.prompts.codex.prompts.is_empty()
            || !self.prompts.gemini.prompts.is_empty()
            || !self.prompts.grokbuild.prompts.is_empty()
            || !self.prompts.opencode.prompts.is_empty()
            || !self.prompts.openclaw.prompts.is_empty()
            || !self.prompts.hermes.prompts.is_empty()
        {
            return Ok(false);
        }

        log::info!("Config file exists and the prompt list is empty; trying to auto-import from existing prompt files");

        let mut imported = false;
        for app in [
            AppType::Claude,
            AppType::Codex,
            AppType::Gemini,
            AppType::GrokBuild,
            AppType::OpenCode,
            AppType::OpenClaw,
            AppType::Hermes,
        ] {
            // Reuse the existing single-app import logic
            if Self::auto_import_prompt_if_exists(self, app)? {
                imported = true;
            }
        }

        Ok(imported)
    }

    /// Check and auto-import a single app's prompt file
    ///
    /// Return value:
    /// - Ok(true)  a non-empty file was imported successfully
    /// - Ok(false) nothing was imported (file missing, empty content, or read failure)
    fn auto_import_prompt_if_exists(config: &mut Self, app: AppType) -> Result<bool, AppError> {
        let file_path = prompt_file_path(&app)?;

        // Check whether the file exists
        if !file_path.exists() {
            log::debug!("Prompt file does not exist; skipping auto-import: {file_path:?}");
            return Ok(false);
        }

        // Read the file content
        let content = match std::fs::read_to_string(&file_path) {
            Ok(c) => c,
            Err(e) => {
                log::warn!("Failed to read the prompt file: {file_path:?}, error: {e}");
                return Ok(false); // Do not abort on failure; continue with the other apps
            }
        };

        // Check whether the content is empty
        if content.trim().is_empty() {
            log::debug!("Prompt file content is empty; skipping import: {file_path:?}");
            return Ok(false);
        }

        log::info!("Prompt file found; auto-importing: {file_path:?}");

        // Create the prompt object
        let timestamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_secs() as i64)
            .unwrap_or_else(|_| {
                log::warn!("Failed to get system time, using 0 as timestamp");
                0
            });

        let id = format!("auto-imported-{timestamp}");
        let prompt = crate::prompt::Prompt {
            id: id.clone(),
            name: format!(
                "Auto-imported Prompt {}",
                chrono::Local::now().format("%Y-%m-%d %H:%M")
            ),
            content,
            description: Some("Automatically imported on first launch".to_string()),
            enabled: true, // Enabled automatically
            created_at: Some(timestamp),
            updated_at: Some(timestamp),
        };

        // Insert into the matching app config
        let prompts = match app {
            AppType::Claude => &mut config.prompts.claude.prompts,
            AppType::ClaudeDesktop => &mut config.prompts.claude_desktop.prompts,
            AppType::Codex => &mut config.prompts.codex.prompts,
            AppType::Gemini => &mut config.prompts.gemini.prompts,
            AppType::GrokBuild => &mut config.prompts.grokbuild.prompts,
            AppType::OpenCode => &mut config.prompts.opencode.prompts,
            AppType::OpenClaw => &mut config.prompts.openclaw.prompts,
            AppType::Hermes => &mut config.prompts.hermes.prompts,
            // Pi was added after prompts moved to SQLite. Keeping it out of
            // this legacy config avoids a second, unused prompt state.
            AppType::Pi => return Ok(false),
        };

        prompts.insert(id, prompt);

        log::info!("Auto-import finished: {}", app.as_str());
        Ok(true)
    }

    /// Migrate the v3.6.x per-app MCP structure to the v3.7.0 unified structure
    ///
    /// Migration strategy:
    /// 1. Check whether the migration already ran (does mcp.servers exist?)
    /// 2. Collect the MCP entries of every app, deduplicating and merging by ID
    /// 3. Produce the unified McpServer structure marking which clients it applies to
    /// 4. Clear the legacy per-app config
    pub fn migrate_mcp_to_unified(&mut self) -> Result<bool, AppError> {
        // Check whether this is already the new structure
        if self.mcp.servers.is_some() {
            log::debug!("MCP config is already the unified structure; skipping the migration");
            return Ok(false);
        }

        log::info!(
            "Legacy MCP config format detected; migrating to the v3.7.0 unified structure..."
        );

        let mut unified_servers: HashMap<String, McpServer> = HashMap::new();
        let mut conflicts = Vec::new();

        // Collect the MCP entries of every app
        for app in [
            AppType::Claude,
            AppType::Codex,
            AppType::Gemini,
            AppType::OpenCode,
        ] {
            let old_servers = match app {
                AppType::Claude => &self.mcp.claude.servers,
                AppType::ClaudeDesktop => continue, // Claude Desktop 3P profiles don't use MCP here
                AppType::Codex => &self.mcp.codex.servers,
                AppType::Gemini => &self.mcp.gemini.servers,
                AppType::GrokBuild => continue,
                AppType::OpenCode => &self.mcp.opencode.servers,
                AppType::OpenClaw => continue, // OpenClaw MCP is still in development, skip
                AppType::Hermes => continue,   // Hermes didn't exist in v3.6.x, skip
                AppType::Pi => continue,       // Pi didn't exist in v3.6.x, skip
            };

            for (id, entry) in old_servers {
                let enabled = entry
                    .get("enabled")
                    .and_then(|v| v.as_bool())
                    .unwrap_or(true);

                if let Some(existing) = unified_servers.get_mut(id) {
                    // This ID already exists, merge the apps field
                    existing.apps.set_enabled_for(&app, enabled);

                    // Detect config conflicts (same ID but different config)
                    if existing.server != *entry.get("server").unwrap_or(&serde_json::json!({})) {
                        conflicts.push(format!(
                            "MCP '{id}' is configured differently in {} than in an earlier app; the first configuration encountered will be used",
                            app.as_str()
                        ));
                    }
                } else {
                    // First time this MCP is seen, create a new entry
                    let name = entry
                        .get("name")
                        .and_then(|v| v.as_str())
                        .unwrap_or(id)
                        .to_string();

                    let server = entry
                        .get("server")
                        .cloned()
                        .unwrap_or(serde_json::json!({}));

                    let description = entry
                        .get("description")
                        .and_then(|v| v.as_str())
                        .map(|s| s.to_string());

                    let homepage = entry
                        .get("homepage")
                        .and_then(|v| v.as_str())
                        .map(|s| s.to_string());

                    let docs = entry
                        .get("docs")
                        .and_then(|v| v.as_str())
                        .map(|s| s.to_string());

                    let tags = entry
                        .get("tags")
                        .and_then(|v| v.as_array())
                        .map(|arr| {
                            arr.iter()
                                .filter_map(|v| v.as_str().map(|s| s.to_string()))
                                .collect()
                        })
                        .unwrap_or_default();

                    let mut apps = McpApps::default();
                    apps.set_enabled_for(&app, enabled);

                    unified_servers.insert(
                        id.clone(),
                        McpServer {
                            id: id.clone(),
                            name,
                            server,
                            apps,
                            description,
                            homepage,
                            docs,
                            tags,
                        },
                    );
                }
            }
        }

        // Log the conflict warnings
        if !conflicts.is_empty() {
            log::warn!("Config conflicts detected during the MCP migration:");
            for conflict in &conflicts {
                log::warn!("  - {conflict}");
            }
        }

        log::info!(
            "MCP migration finished, {} server(s) migrated{}",
            unified_servers.len(),
            if !conflicts.is_empty() {
                format!(" ({} conflict(s))", conflicts.len())
            } else {
                String::new()
            }
        );

        // Replace with the new structure
        self.mcp.servers = Some(unified_servers);

        // Clear the legacy per-app config
        self.mcp.claude = McpConfig::default();
        self.mcp.codex = McpConfig::default();
        self.mcp.gemini = McpConfig::default();

        Ok(true)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serial_test::serial;
    use std::env;
    use std::fs;
    use tempfile::TempDir;

    #[test]
    fn app_type_parses_claude_desktop_aliases() {
        assert_eq!(
            "claude-desktop".parse::<AppType>().unwrap(),
            AppType::ClaudeDesktop
        );
        assert_eq!(
            "claude_desktop".parse::<AppType>().unwrap(),
            AppType::ClaudeDesktop
        );
        assert_eq!(
            "claudeDesktop".parse::<AppType>().unwrap(),
            AppType::ClaudeDesktop
        );
        assert_eq!(AppType::ClaudeDesktop.as_str(), "claude-desktop");
    }

    struct TempHome {
        #[allow(dead_code)] // RAII guard: Drop keeps the temp dir alive for the whole test
        dir: TempDir,
        original_home: Option<String>,
        original_userprofile: Option<String>,
        original_test_home: Option<String>,
    }

    impl TempHome {
        fn new() -> Self {
            let dir = TempDir::new().expect("failed to create temp home");
            let original_home = env::var("HOME").ok();
            let original_userprofile = env::var("USERPROFILE").ok();
            let original_test_home = env::var("AI_MANAGER_TEST_HOME").ok();

            env::set_var("HOME", dir.path());
            env::set_var("USERPROFILE", dir.path());
            env::set_var("AI_MANAGER_TEST_HOME", dir.path());

            Self {
                dir,
                original_home,
                original_userprofile,
                original_test_home,
            }
        }
    }

    impl Drop for TempHome {
        fn drop(&mut self) {
            match &self.original_home {
                Some(value) => env::set_var("HOME", value),
                None => env::remove_var("HOME"),
            }

            match &self.original_userprofile {
                Some(value) => env::set_var("USERPROFILE", value),
                None => env::remove_var("USERPROFILE"),
            }

            match &self.original_test_home {
                Some(value) => env::set_var("AI_MANAGER_TEST_HOME", value),
                None => env::remove_var("AI_MANAGER_TEST_HOME"),
            }
        }
    }

    fn write_prompt_file(app: AppType, content: &str) {
        let path = crate::prompt_files::prompt_file_path(&app).expect("prompt path");
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).expect("create parent dir");
        }
        fs::write(path, content).expect("write prompt");
    }

    #[test]
    #[serial]
    fn unsupported_v1_error_names_the_resolved_config_path() {
        let _home = TempHome::new();
        let config_path = crate::config::get_app_config_path();
        fs::create_dir_all(config_path.parent().expect("config parent"))
            .expect("create config parent");
        fs::write(&config_path, r#"{"providers":{},"current":"legacy"}"#)
            .expect("write legacy config");

        let error = MultiAppConfig::load().expect_err("v1 config must be rejected");
        let rendered = error.to_string();
        assert!(rendered.contains(&config_path.display().to_string()));
        assert!(!rendered.contains("~/.cc-switch/config.json"));
    }

    #[test]
    #[serial]
    fn auto_imports_existing_prompt_when_config_missing() {
        let _home = TempHome::new();
        write_prompt_file(AppType::Claude, "# hello");

        let config = MultiAppConfig::load().expect("load config");

        assert_eq!(config.prompts.claude.prompts.len(), 1);
        let prompt = config
            .prompts
            .claude
            .prompts
            .values()
            .next()
            .expect("prompt exists");
        assert!(prompt.enabled);
        assert_eq!(prompt.content, "# hello");

        let config_path = crate::config::get_app_config_path();
        assert!(
            config_path.exists(),
            "auto import should persist config to disk"
        );
    }

    #[test]
    #[serial]
    fn skips_empty_prompt_files_during_import() {
        let _home = TempHome::new();
        write_prompt_file(AppType::Claude, "   \n  ");

        let config = MultiAppConfig::load().expect("load config");
        assert!(
            config.prompts.claude.prompts.is_empty(),
            "empty files must be ignored"
        );
    }

    #[test]
    #[serial]
    fn auto_import_happens_only_once() {
        let _home = TempHome::new();
        write_prompt_file(AppType::Claude, "first version");

        let first = MultiAppConfig::load().expect("load config");
        assert_eq!(first.prompts.claude.prompts.len(), 1);
        let claude_prompt = first
            .prompts
            .claude
            .prompts
            .values()
            .next()
            .expect("prompt exists")
            .content
            .clone();
        assert_eq!(claude_prompt, "first version");

        // Overwrite the file contents but keep config.json
        write_prompt_file(AppType::Claude, "second version");
        let second = MultiAppConfig::load().expect("load config again");

        assert_eq!(second.prompts.claude.prompts.len(), 1);
        let prompt = second
            .prompts
            .claude
            .prompts
            .values()
            .next()
            .expect("prompt exists");
        assert_eq!(
            prompt.content, "first version",
            "should not re-import when config already exists"
        );
    }

    #[test]
    #[serial]
    fn auto_imports_gemini_prompt_on_first_launch() {
        let _home = TempHome::new();
        write_prompt_file(AppType::Gemini, "# Gemini Prompt\n\nTest content");

        let config = MultiAppConfig::load().expect("load config");

        assert_eq!(config.prompts.gemini.prompts.len(), 1);
        let prompt = config
            .prompts
            .gemini
            .prompts
            .values()
            .next()
            .expect("gemini prompt exists");
        assert!(prompt.enabled, "gemini prompt should be enabled");
        assert_eq!(prompt.content, "# Gemini Prompt\n\nTest content");
        assert_eq!(
            prompt.description,
            Some("Automatically imported on first launch".to_string())
        );
    }

    #[test]
    #[serial]
    fn auto_imports_grokbuild_prompt_on_first_launch() {
        let _home = TempHome::new();
        write_prompt_file(AppType::GrokBuild, "# Grok Build Prompt\n\nTest content");

        let config = MultiAppConfig::load().expect("load config");

        assert_eq!(config.prompts.grokbuild.prompts.len(), 1);
        let prompt = config
            .prompts
            .grokbuild
            .prompts
            .values()
            .next()
            .expect("grokbuild prompt exists");
        assert!(prompt.enabled, "grokbuild prompt should be enabled");
        assert_eq!(prompt.content, "# Grok Build Prompt\n\nTest content");
        assert_eq!(
            prompt.description,
            Some("Automatically imported on first launch".to_string())
        );
    }

    #[test]
    #[serial]
    fn auto_imports_all_three_apps_prompts() {
        let _home = TempHome::new();
        write_prompt_file(AppType::Claude, "# Claude prompt");
        write_prompt_file(AppType::Codex, "# Codex prompt");
        write_prompt_file(AppType::Gemini, "# Gemini prompt");

        let config = MultiAppConfig::load().expect("load config");

        // Verify the prompts of all three apps were imported
        assert_eq!(config.prompts.claude.prompts.len(), 1);
        assert_eq!(config.prompts.codex.prompts.len(), 1);
        assert_eq!(config.prompts.gemini.prompts.len(), 1);

        // Verify every prompt is enabled
        assert!(
            config
                .prompts
                .claude
                .prompts
                .values()
                .next()
                .unwrap()
                .enabled
        );
        assert!(
            config
                .prompts
                .codex
                .prompts
                .values()
                .next()
                .unwrap()
                .enabled
        );
        assert!(
            config
                .prompts
                .gemini
                .prompts
                .values()
                .next()
                .unwrap()
                .enabled
        );
    }
}
