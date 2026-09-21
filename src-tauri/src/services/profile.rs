//! Project profile orchestration service
//!
//! A Profile is a **project entity shared by all apps** (users only ever have a handful of projects);
//! its payload stores per-app config snapshots (provider / MCP / Skills / Prompt). Snapshotting and
//! applying both work **per scope**: Claude Code and Codex usually sit in different working directories
//! (each inside its own project), so every scope points at its own current project and only captures or
//! applies its own slots; renaming/deleting acts on the shared entity itself.
//! Applying reuses the existing switch primitives in bulk:
//! - Provider: `ProviderService::switch` (built-in proxy-takeover hot switch, official switch blocked under takeover)
//! - MCP: `McpService::toggle_app` (flip the flag + materialize the single server)
//! - Skills: `SkillService::toggle_app` (flip the flag + materialize the single skill)
//! - Prompt: `PromptService::enable_prompt` (mutually exclusive activation + atomic live write)
//!
//! apply is best-effort: a single failure is collected as a warning and nothing is rolled back.

use std::collections::HashSet;

use serde::{Deserialize, Serialize};

use crate::app_config::AppType;
use crate::database::Profile;
use crate::error::AppError;
use crate::services::{McpService, PromptService, ProviderService, SkillService};
use crate::store::AppState;

/// App scope for profile operations: the project entity is shared by all apps, but snapshot, apply
/// and the current pointer all work per scope.
///
/// In cc-switch the Claude Code and Claude Desktop providers switch independently, so each owns its own
/// project scope. Their live files have zero overlap
/// (`~/.claude` / `Application Support/Claude-3p`), so scope switches do not interfere.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ProfileScope {
    Claude,
    #[serde(rename = "claude-desktop")]
    ClaudeDesktop,
    Codex,
}

impl ProfileScope {
    /// All scopes (when adding one, extend apps/for_app and the frontend scope.ts mirror as well)
    pub const ALL: [ProfileScope; 3] = [
        ProfileScope::Claude,
        ProfileScope::ClaudeDesktop,
        ProfileScope::Codex,
    ];

    pub fn as_str(&self) -> &'static str {
        match self {
            ProfileScope::Claude => "claude",
            ProfileScope::ClaudeDesktop => "claude-desktop",
            ProfileScope::Codex => "codex",
        }
    }

    pub fn parse(value: &str) -> Result<Self, AppError> {
        match value {
            "claude" => Ok(ProfileScope::Claude),
            "claude-desktop" => Ok(ProfileScope::ClaudeDesktop),
            "codex" => Ok(ProfileScope::Codex),
            other => Err(AppError::InvalidInput(format!(
                "Unknown profile scope: {other}"
            ))),
        }
    }

    /// Apps managed by this scope (snapshot and apply only touch these apps' slots)
    pub fn apps(&self) -> &'static [AppType] {
        match self {
            ProfileScope::Claude => &[AppType::Claude],
            ProfileScope::ClaudeDesktop => &[AppType::ClaudeDesktop],
            ProfileScope::Codex => &[AppType::Codex],
        }
    }

    /// App page -> owning scope (returns None for apps Profile does not support)
    pub fn for_app(app: &AppType) -> Option<Self> {
        match app {
            AppType::Claude => Some(ProfileScope::Claude),
            AppType::ClaudeDesktop => Some(ProfileScope::ClaudeDesktop),
            AppType::Codex => Some(ProfileScope::Codex),
            _ => None,
        }
    }
}

/// Per-app slot container; the field names match the serde form of AppType
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct PerApp<T> {
    pub claude: T,
    #[serde(rename = "claude-desktop")]
    pub claude_desktop: T,
    pub codex: T,
}

impl<T> PerApp<T> {
    pub fn get(&self, app: &AppType) -> Option<&T> {
        match app {
            AppType::Claude => Some(&self.claude),
            AppType::ClaudeDesktop => Some(&self.claude_desktop),
            AppType::Codex => Some(&self.codex),
            _ => None,
        }
    }

    pub fn get_mut(&mut self, app: &AppType) -> Option<&mut T> {
        match app {
            AppType::Claude => Some(&mut self.claude),
            AppType::ClaudeDesktop => Some(&mut self.claude_desktop),
            AppType::Codex => Some(&mut self.codex),
            _ => None,
        }
    }
}

/// JSON snapshot structure of a Profile (mirrors the frontend TS type exactly)
///
/// Every slot is an Option: None = that side was never snapshotted (apply leaves it untouched),
/// strictly distinct from "captured an empty set / no active item" (Some(empty), apply clears them) -
/// picking a project created only on the Claude page must not wipe Codex's enabled state.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct ProfilePayload {
    /// Current provider id per app
    pub providers: PerApp<Option<String>>,
    /// Set of enabled MCP server ids per app
    pub mcp: PerApp<Option<Vec<String>>>,
    /// Set of enabled Skill ids per app
    pub skills: PerApp<Option<Vec<String>>>,
    /// Active prompt id per app
    pub prompts: PerApp<Option<String>>,
}

impl ProfilePayload {
    /// Overwrite the slots of one scope in this payload from another snapshot, leaving other scopes as-is
    /// ("update with current state" only updates the scope of the initiating page, so other apps sitting
    /// in a different project do not leak in)
    pub fn merge_scope_from(&mut self, other: &ProfilePayload, scope: ProfileScope) {
        for app in scope.apps() {
            if let (Some(dst), Some(src)) = (self.providers.get_mut(app), other.providers.get(app))
            {
                *dst = src.clone();
            }
            if let (Some(dst), Some(src)) = (self.mcp.get_mut(app), other.mcp.get(app)) {
                *dst = src.clone();
            }
            if let (Some(dst), Some(src)) = (self.skills.get_mut(app), other.skills.get(app)) {
                *dst = src.clone();
            }
            if let (Some(dst), Some(src)) = (self.prompts.get_mut(app), other.prompts.get(app)) {
                *dst = src.clone();
            }
        }
    }

    /// Whether a scope has been snapshotted (any non-None slot counts as captured)
    pub fn scope_captured(&self, scope: ProfileScope) -> bool {
        scope.apps().iter().any(|app| {
            self.providers.get(app).is_some_and(|s| s.is_some())
                || self.mcp.get(app).is_some_and(|s| s.is_some())
                || self.skills.get(app).is_some_and(|s| s.is_some())
                || self.prompts.get(app).is_some_and(|s| s.is_some())
        })
    }
}

/// Compute the minimal toggle set to go from the current enabled state to the target set
///
/// Returns (the (id, enabled) entries to execute, the dangling ids present in the payload but not in the DB)
fn plan_toggles(
    current: &[(String, bool)],
    target_ids: &[String],
) -> (Vec<(String, bool)>, Vec<String>) {
    let existing: HashSet<&str> = current.iter().map(|(id, _)| id.as_str()).collect();
    let target: HashSet<&str> = target_ids.iter().map(|s| s.as_str()).collect();

    let toggles = current
        .iter()
        .filter(|(id, enabled)| target.contains(id.as_str()) != *enabled)
        .map(|(id, enabled)| (id.clone(), !enabled))
        .collect();

    let dangling = target_ids
        .iter()
        .filter(|id| !existing.contains(id.as_str()))
        .cloned()
        .collect();

    (toggles, dangling)
}

pub struct ProfileService;

impl ProfileService {
    /// Capture the current config state of the scope's apps into a snapshot (out-of-scope slots keep defaults)
    pub fn snapshot_current(
        state: &AppState,
        scope: ProfileScope,
    ) -> Result<ProfilePayload, AppError> {
        let mut payload = ProfilePayload::default();
        let mcp_servers = state.db.get_all_mcp_servers()?;
        let skills = state.db.get_all_installed_skills()?;

        for app in scope.apps().iter() {
            if let Some(slot) = payload.providers.get_mut(app) {
                *slot = crate::settings::get_effective_current_provider(&state.db, app)?;
            }
            if let Some(slot) = payload.mcp.get_mut(app) {
                *slot = Some(
                    mcp_servers
                        .values()
                        .filter(|s| s.apps.is_enabled_for(app))
                        .map(|s| s.id.clone())
                        .collect(),
                );
            }
            if let Some(slot) = payload.skills.get_mut(app) {
                *slot = Some(
                    skills
                        .values()
                        .filter(|s| s.apps.is_enabled_for(app))
                        .map(|s| s.id.clone())
                        .collect(),
                );
            }
            if let Some(slot) = payload.prompts.get_mut(app) {
                *slot = state
                    .db
                    .get_prompts(app.as_str())?
                    .values()
                    .find(|p| p.enabled)
                    .map(|p| p.id.clone());
            }
        }
        Ok(payload)
    }

    /// List all projects (the project entity is shared by all apps; the current marker is read per scope)
    pub fn list(state: &AppState) -> Result<Vec<Profile>, AppError> {
        state.db.get_all_profiles()
    }

    /// Create a project: snapshot only the current state of the initiating page's scope, leaving the other
    /// scope slots as None (other apps may sit in a different project, so do not capture that for the user)
    pub fn create(state: &AppState, name: &str, scope: ProfileScope) -> Result<Profile, AppError> {
        let name = name.trim();
        if name.is_empty() {
            return Err(AppError::InvalidInput("Profile name is empty".to_string()));
        }
        let payload = Self::snapshot_current(state, scope)?;
        let now = chrono::Utc::now().timestamp();
        let profile = Profile {
            id: uuid::Uuid::new_v4().to_string(),
            name: name.to_string(),
            payload: serde_json::to_string(&payload).map_err(|e| {
                AppError::Config(format!("Failed to serialize profile payload: {e}"))
            })?,
            sort_order: None,
            created_at: Some(now),
            updated_at: Some(now),
        };
        state.db.save_profile(&profile)?;
        Ok(profile)
    }

    /// Update a project: rename (acts on the shared entity) and/or re-snapshot from the current state
    /// (a resnapshot only overwrites the scope's slots, other scopes stay as-is; re-snapshotting is only
    /// triggered by the auto-save in [`Self::apply`] before a switch, the UI no longer exposes it)
    pub fn update(
        state: &AppState,
        id: &str,
        name: Option<String>,
        resnapshot: bool,
        scope: Option<ProfileScope>,
    ) -> Result<Profile, AppError> {
        let mut profile = state
            .db
            .get_profile(id)?
            .ok_or_else(|| AppError::InvalidInput(format!("Profile not found: {id}")))?;

        if let Some(name) = name {
            let name = name.trim().to_string();
            if name.is_empty() {
                return Err(AppError::InvalidInput("Profile name is empty".to_string()));
            }
            profile.name = name;
        }
        if resnapshot {
            let scope = scope.ok_or_else(|| {
                AppError::InvalidInput("Resnapshot requires a profile scope".to_string())
            })?;
            let mut payload: ProfilePayload = serde_json::from_str(&profile.payload)
                .map_err(|e| AppError::Config(format!("Failed to parse profile payload: {e}")))?;
            payload.merge_scope_from(&Self::snapshot_current(state, scope)?, scope);
            profile.payload = serde_json::to_string(&payload).map_err(|e| {
                AppError::Config(format!("Failed to serialize profile payload: {e}"))
            })?;
        }
        profile.updated_at = Some(chrono::Utc::now().timestamp());
        state.db.save_profile(&profile)?;
        Ok(profile)
    }

    /// Delete a project; if it is the active project of a scope, clear that scope's active marker too
    pub fn delete(state: &AppState, id: &str) -> Result<(), AppError> {
        state.db.delete_profile(id)?;
        for scope in ProfileScope::ALL {
            if state.db.get_current_profile_id(scope.as_str())?.as_deref() == Some(id) {
                state.db.set_current_profile_id(scope.as_str(), None)?;
            }
        }
        Ok(())
    }

    /// Apply a project snapshot (best-effort, returns warnings)
    ///
    /// Only affects apps inside the initiating page's scope; other scopes' config and current markers are untouched.
    /// If the scope was never snapshotted no config is changed; it is only marked current and a hint is returned
    /// (the auto-save will capture that side the next time you switch away from this project).
    ///
    /// **The previous project is auto-saved before switching**: if the current scope is already bound to
    /// another project, the current state is written into that old project first (current scope slots only),
    /// then the target project is loaded. The old project therefore keeps the config it had when you left.
    /// A failed auto-save is reported as a warning and does not block the switch.
    ///
    /// Apply the snapshot of the given project to every app in the current scope.
    ///
    /// Returns `(warnings, should_stop_proxy)`: when every takeover in the current scope is off and no
    /// other app has a takeover either, the caller is advised to stop the proxy service so the
    /// "local routing" master switch of Claude Desktop shows as off as well.
    pub fn apply(
        state: &AppState,
        profile_id: &str,
        scope: ProfileScope,
    ) -> Result<(Vec<String>, bool), AppError> {
        let mut warnings = Vec::new();

        // Auto-save the current state of the old project (current scope only); a failure does not block the switch
        if let Some(current_id) = state.db.get_current_profile_id(scope.as_str())? {
            if current_id != profile_id {
                if let Err(e) = Self::update(state, &current_id, None, true, Some(scope)) {
                    warnings.push(format!(
                        "autosave profile '{current_id}' before switch failed: {e}"
                    ));
                }
            }
        }

        let profile = state
            .db
            .get_profile(profile_id)?
            .ok_or_else(|| AppError::InvalidInput(format!("Profile not found: {profile_id}")))?;
        let payload: ProfilePayload = serde_json::from_str(&profile.payload)
            .map_err(|e| AppError::Config(format!("Failed to parse profile payload: {e}")))?;

        if !payload.scope_captured(scope) {
            warnings.push(format!(
                "no {} configuration captured in this project yet; marked as current without changes (it will be saved automatically when you switch away)",
                scope.as_str()
            ));
        }

        for app in scope.apps().iter() {
            let app_str = app.as_str();

            // 1. Unconditionally disable the current app's proxy takeover before switching projects.
            // Under takeover the live file belongs to the proxy; when switching working directory the user
            // always wants to leave the proxy environment and write the real provider config from the snapshot.
            if let Err(e) = state.proxy_service.disable_takeover_for_app_sync(app) {
                warnings.push(format!(
                    "[{app_str}] auto-disable proxy takeover before profile switch failed: {e}"
                ));
            }

            // 2. Provider
            if let Some(Some(target_pid)) = payload.providers.get(app) {
                let providers = state.db.get_all_providers(app_str)?;
                if !providers.contains_key(target_pid) {
                    warnings.push(format!(
                        "[{app_str}] provider '{target_pid}' no longer exists, skipped"
                    ));
                } else {
                    let current = crate::settings::get_effective_current_provider(&state.db, app)?;
                    if current.as_deref() != Some(target_pid.as_str()) {
                        match ProviderService::switch(state, app.clone(), target_pid) {
                            Ok(result) => warnings.extend(result.warnings),
                            Err(e) => warnings.push(format!(
                                "[{app_str}] switch provider '{target_pid}' failed: {e}"
                            )),
                        }
                    }
                }
            }

            // 3. MCP diff (minimal toggles: only entries whose target differs from the current state; None = never snapshotted, untouched)
            if let Some(Some(target_ids)) = payload.mcp.get(app) {
                let servers = state.db.get_all_mcp_servers()?;
                let current: Vec<(String, bool)> = servers
                    .values()
                    .map(|s| (s.id.clone(), s.apps.is_enabled_for(app)))
                    .collect();
                let (toggles, dangling) = plan_toggles(&current, target_ids);
                for id in dangling {
                    warnings.push(format!("[{app_str}] MCP '{id}' no longer exists, skipped"));
                }
                for (id, enabled) in toggles {
                    if let Err(e) = McpService::toggle_app(state, &id, app.clone(), enabled) {
                        warnings.push(format!(
                            "[{app_str}] toggle MCP '{id}' -> {enabled} failed: {e}"
                        ));
                    }
                }
            }

            // 4. Skills diff (SkillService returns anyhow::Result, folded into warnings)
            if let Some(Some(target_ids)) = payload.skills.get(app) {
                let skills = state.db.get_all_installed_skills()?;
                let current: Vec<(String, bool)> = skills
                    .values()
                    .map(|s| (s.id.clone(), s.apps.is_enabled_for(app)))
                    .collect();
                let (toggles, dangling) = plan_toggles(&current, target_ids);
                for id in dangling {
                    warnings.push(format!(
                        "[{app_str}] skill '{id}' no longer exists, skipped"
                    ));
                }
                for (id, enabled) in toggles {
                    if let Err(e) = SkillService::toggle_app(&state.db, &id, app, enabled) {
                        warnings.push(format!(
                            "[{app_str}] toggle skill '{id}' -> {enabled} failed: {e}"
                        ));
                    }
                }
            }

            // 5. Prompt (None = untouched; an already active prompt is skipped idempotently to avoid a pointless file write and backup)
            if let Some(Some(target_prompt)) = payload.prompts.get(app) {
                let prompts = state.db.get_prompts(app_str)?;
                match prompts.get(target_prompt) {
                    None => warnings.push(format!(
                        "[{app_str}] prompt '{target_prompt}' no longer exists, skipped"
                    )),
                    Some(p) if p.enabled => {}
                    Some(_) => {
                        if let Err(e) =
                            PromptService::enable_prompt(state, app.clone(), target_prompt)
                        {
                            warnings.push(format!(
                                "[{app_str}] enable prompt '{target_prompt}' failed: {e}"
                            ));
                        }
                    }
                }
            }
        }

        state
            .db
            .set_current_profile_id(scope.as_str(), Some(profile_id))?;

        // Every takeover in the current scope is off; if no other app has one either, the proxy service can stop.
        let should_stop_proxy = !state.db.is_live_takeover_active_sync();

        Ok((warnings, should_stop_proxy))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ids(v: &[&str]) -> Vec<String> {
        v.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn test_payload_serde_roundtrip() {
        let payload = ProfilePayload {
            providers: PerApp {
                claude: Some("p1".into()),
                claude_desktop: Some("d1".into()),
                codex: None,
            },
            mcp: PerApp {
                claude: Some(ids(&["m1", "m2"])),
                claude_desktop: Some(vec![]),
                codex: None,
            },
            skills: PerApp {
                claude: Some(vec![]),
                claude_desktop: Some(vec![]),
                codex: Some(ids(&["s1"])),
            },
            prompts: PerApp {
                claude: None,
                claude_desktop: None,
                codex: Some("pr1".into()),
            },
        };
        let json = serde_json::to_string(&payload).unwrap();
        // per-app keys must match the serde form of AppType (claude-desktop uses a hyphen)
        assert!(json.contains("\"claude\""));
        assert!(json.contains("\"claude-desktop\""));
        assert!(json.contains("\"codex\""));
        let back: ProfilePayload = serde_json::from_str(&json).unwrap();
        assert_eq!(back, payload);
    }

    #[test]
    fn test_payload_tolerates_missing_fields() {
        // Forward compatibility: older versions / missing fields must fall back to None ("never snapshotted")
        // instead of erroring; apply leaves missing slots untouched
        let back: ProfilePayload =
            serde_json::from_str(r#"{"providers":{"claude":"p1"},"mcp":{"claude":["m1"]}}"#)
                .unwrap();
        assert_eq!(back.providers.claude, Some("p1".to_string()));
        assert_eq!(back.providers.claude_desktop, None);
        assert_eq!(back.providers.codex, None);
        assert_eq!(back.mcp.claude, Some(ids(&["m1"])));
        assert_eq!(back.mcp.claude_desktop, None);
        assert_eq!(back.mcp.codex, None, "missing slot means untouched");
        assert_eq!(back.prompts.codex, None);

        let empty: ProfilePayload = serde_json::from_str("{}").unwrap();
        assert_eq!(empty, ProfilePayload::default());
    }

    #[test]
    fn test_merge_scope_from_only_touches_scope_slots() {
        // Project A: both sides already snapshotted
        let mut payload = ProfilePayload {
            providers: PerApp {
                claude: Some("p1".into()),
                claude_desktop: Some("d1".into()),
                codex: Some("c1".into()),
            },
            mcp: PerApp {
                claude: Some(ids(&["m1"])),
                claude_desktop: Some(vec![]),
                codex: Some(ids(&["m9"])),
            },
            ..Default::default()
        };
        // "Update with current state" on the Claude page: only the claude scope slots are overwritten
        let fresh = ProfilePayload {
            providers: PerApp {
                claude: Some("p2".into()),
                claude_desktop: None,
                codex: Some("SHOULD-NOT-LEAK".into()),
            },
            mcp: PerApp {
                claude: Some(ids(&["m2"])),
                claude_desktop: Some(vec![]),
                codex: None,
            },
            ..Default::default()
        };
        payload.merge_scope_from(&fresh, ProfileScope::Claude);

        assert_eq!(payload.providers.claude, Some("p2".to_string()));
        assert_eq!(
            payload.providers.claude_desktop,
            Some("d1".to_string()),
            "claude-desktop slot is in its own scope, untouched by claude merge"
        );
        assert_eq!(payload.mcp.claude, Some(ids(&["m2"])));
        // codex side intact: neither overwritten nor polluted by the fresh values
        assert_eq!(payload.providers.codex, Some("c1".to_string()));
        assert_eq!(payload.mcp.codex, Some(ids(&["m9"])));
    }

    #[test]
    fn test_scope_captured_detects_per_scope_snapshot() {
        let mut payload = ProfilePayload::default();
        assert!(!payload.scope_captured(ProfileScope::Claude));
        assert!(!payload.scope_captured(ProfileScope::ClaudeDesktop));
        assert!(!payload.scope_captured(ProfileScope::Codex));

        // Only the claude scope was snapshotted (even if what was captured is an empty set)
        payload.mcp.claude = Some(vec![]);
        assert!(payload.scope_captured(ProfileScope::Claude));
        assert!(!payload.scope_captured(ProfileScope::ClaudeDesktop));
        assert!(!payload.scope_captured(ProfileScope::Codex));

        // The Desktop slot belongs to the separate claude-desktop scope
        let mut desktop_only = ProfilePayload::default();
        desktop_only.providers.claude_desktop = Some("d1".into());
        assert!(desktop_only.scope_captured(ProfileScope::ClaudeDesktop));
        assert!(!desktop_only.scope_captured(ProfileScope::Claude));
    }

    #[test]
    fn test_per_app_get_only_supports_profile_apps() {
        let per: PerApp<Option<String>> = PerApp::default();
        assert!(per.get(&AppType::Claude).is_some());
        assert!(per.get(&AppType::ClaudeDesktop).is_some());
        assert!(per.get(&AppType::Codex).is_some());
        assert!(per.get(&AppType::Gemini).is_none());
    }

    #[test]
    fn test_scope_serde_and_parse_roundtrip() {
        for scope in ProfileScope::ALL {
            // The DB storage string (as_str/parse) and the JSON serialization must use the same form
            assert_eq!(
                serde_json::to_string(&scope).unwrap(),
                format!("\"{}\"", scope.as_str())
            );
            assert_eq!(ProfileScope::parse(scope.as_str()).unwrap(), scope);
        }
        assert!(ProfileScope::parse("gemini").is_err());
        assert!(ProfileScope::parse("").is_err());
    }

    #[test]
    fn test_scope_app_grouping() {
        // Claude Code and Claude Desktop each form their own scope;
        // the scope's apps and the for_app reverse mapping must agree
        assert_eq!(ProfileScope::Claude.apps(), &[AppType::Claude]);
        assert_eq!(
            ProfileScope::ClaudeDesktop.apps(),
            &[AppType::ClaudeDesktop]
        );
        assert_eq!(ProfileScope::Codex.apps(), &[AppType::Codex]);
        for scope in ProfileScope::ALL {
            for app in scope.apps() {
                assert_eq!(ProfileScope::for_app(app), Some(scope));
            }
        }
        assert_eq!(ProfileScope::for_app(&AppType::Gemini), None);
    }

    #[test]
    fn test_plan_toggles_minimal_diff() {
        let current = vec![
            ("a".to_string(), true),  // target contains a: no change
            ("b".to_string(), false), // target contains b: enable
            ("c".to_string(), true),  // target lacks c: disable
            ("d".to_string(), false), // target lacks d: no change
        ];
        let (toggles, dangling) = plan_toggles(&current, &ids(&["a", "b", "ghost"]));
        assert_eq!(
            toggles,
            vec![("b".to_string(), true), ("c".to_string(), false)]
        );
        assert_eq!(dangling, ids(&["ghost"]));
    }

    #[test]
    fn test_plan_toggles_empty_target_disables_all_enabled() {
        let current = vec![("a".to_string(), true), ("b".to_string(), false)];
        let (toggles, dangling) = plan_toggles(&current, &[]);
        assert_eq!(toggles, vec![("a".to_string(), false)]);
        assert!(dangling.is_empty());
    }
}
