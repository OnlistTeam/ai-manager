//! Read projection for skills.
//!
//! Isomorphic to MCP: one global record plus a per-tool switch (`SkillApps`,
//! `app_config.rs:90-107`). The only difference is that this upstream side uses `anyhow::Result`,
//! and the errors are funnelled here into a product `AppError` by `list_failed`.

use std::path::{Path, PathBuf};

use crate::app_config::{AppType, InstalledSkill, SkillApps, UnmanagedSkill};
use crate::domain::{
    AppError, Extension, ExtensionKind, ExtensionManagement, ExtensionScope, ToolId,
};
use crate::services::skill::{skill_state_write_guard, ImportSkillSelection};
use crate::services::SkillService;
use crate::store::AppState;

use super::{
    adopt_failed, copy_failed, list_failed, non_empty, not_found, resource_open_failed,
    toggle_failed,
};

const SHARED_SKILL_SOURCES: [&str; 2] = ["agents", "cc-switch"];

/// A Skill in a tool directory belongs only to that tool. A Skill found in
/// the Agent Skills standard directory or the product SSOT is shared local
/// inventory and must not become invisible merely because it has no tool label
/// yet.
pub(super) fn visible_in_scope(raw: &UnmanagedSkill, source: &str) -> bool {
    raw.found_in
        .iter()
        .any(|found| found == source || SHARED_SKILL_SOURCES.contains(&found.as_str()))
}

pub(super) fn extension_from_skill(
    tool: ToolId,
    raw: &InstalledSkill,
    app_type: &AppType,
) -> Extension {
    Extension {
        kind: ExtensionKind::Skill,
        id: raw.id.clone(),
        scope: ExtensionScope::tool(tool),
        name: raw.name.clone(),
        description: non_empty(raw.description.clone()),
        management: ExtensionManagement::Managed,
        enabled: raw.apps.is_enabled_for(app_type),
        can_disable: true,
    }
}

pub(super) fn extension_from_unmanaged(tool: ToolId, raw: &UnmanagedSkill) -> Extension {
    Extension {
        kind: ExtensionKind::Skill,
        id: raw.directory.clone(),
        scope: ExtensionScope::tool(tool),
        name: raw.name.clone(),
        description: non_empty(raw.description.clone()),
        management: ExtensionManagement::Detected,
        // Presence in this tool's directory is the only status asserted here.
        enabled: true,
        // AI Manager must not delete or rewrite a copy it does not own.
        can_disable: false,
    }
}

pub(super) fn list(
    state: &AppState,
    tool: ToolId,
    app_type: &AppType,
) -> Result<Vec<Extension>, AppError> {
    let raw = SkillService::get_all_installed(&state.db).map_err(list_failed)?;
    let mut projected = raw
        .iter()
        .map(|entry| extension_from_skill(tool, entry, app_type))
        .collect::<Vec<_>>();

    let source = app_type.as_str();
    let mut detected = SkillService::scan_unmanaged(&state.db)
        .map_err(list_failed)?
        .into_iter()
        .filter(|entry| visible_in_scope(entry, source))
        .map(|entry| extension_from_unmanaged(tool, &entry))
        .collect::<Vec<_>>();
    detected.sort_by_cached_key(|entry| (entry.name.to_lowercase(), entry.id.clone()));
    projected.extend(detected);
    Ok(projected)
}

fn is_skill_directory(path: &Path, directory: &str) -> bool {
    path.file_name().and_then(|name| name.to_str()) == Some(directory)
        && path.is_dir()
        && path.join("SKILL.md").is_file()
}

fn first_existing_skill_path(
    entry: &UnmanagedSkill,
    current_source: &str,
    current_root: Option<PathBuf>,
    agents_root: PathBuf,
    ssot_root: Option<PathBuf>,
) -> Option<PathBuf> {
    let mut candidates = Vec::new();
    if entry.found_in.iter().any(|found| found == current_source) {
        if let Some(root) = current_root {
            candidates.push(root.join(&entry.directory));
        }
    }
    if entry.found_in.iter().any(|found| found == "agents") {
        candidates.push(agents_root.join(&entry.directory));
    }
    if entry.found_in.iter().any(|found| found == "cc-switch") {
        if let Some(root) = ssot_root {
            candidates.push(root.join(&entry.directory));
        }
    }

    candidates
        .into_iter()
        .find(|path| is_skill_directory(path, &entry.directory))
}

pub(super) fn detected_path(
    state: &AppState,
    scope: ExtensionScope,
    app_type: &AppType,
    id: &str,
) -> Result<PathBuf, AppError> {
    let source = app_type.as_str();
    let detected = SkillService::scan_unmanaged(&state.db).map_err(resource_open_failed)?;
    let entry = detected
        .iter()
        .find(|entry| entry.directory == id && visible_in_scope(entry, source))
        .ok_or_else(|| not_found(scope, ExtensionKind::Skill, id))?;

    let ssot_root = entry
        .found_in
        .iter()
        .any(|found| found == "cc-switch")
        .then(|| SkillService::get_ssot_dir().ok())
        .flatten();
    first_existing_skill_path(
        entry,
        source,
        SkillService::get_app_skills_dir(app_type).ok(),
        crate::config::get_home_dir().join(".agents").join("skills"),
        ssot_root,
    )
    .ok_or_else(|| resource_open_failed("detected Skill no longer has a readable SKILL.md"))
}

/// Copy one detected Skill from the scope it was found in into another tool's
/// Skills directory. The source path is re-resolved from the live inventory by
/// `detected_path`, which deliberately ignores `UnmanagedSkill::path`, so no
/// renderer-supplied location can reach the filesystem.
pub(super) fn copy_detected_to(
    state: &AppState,
    scope: ExtensionScope,
    source_app: &AppType,
    target_app: &AppType,
    id: &str,
) -> Result<(), AppError> {
    let source = detected_path(state, scope, source_app, id)?;
    let _state_guard = skill_state_write_guard();
    SkillService::copy_detected_to_app_dir(&source, id, target_app).map_err(copy_failed)
}

pub(super) fn set_enabled(
    state: &AppState,
    app_type: &AppType,
    id: &str,
    enabled: bool,
) -> Result<(), AppError> {
    // Enabling copies the skill directory from the SSOT directory into that tool's skills directory
    // and disabling removes it again (services/skill.rs:1838-1853).
    SkillService::toggle_app(&state.db, id, app_type, enabled).map_err(toggle_failed)
}

pub(super) fn adoption_scopes(
    entry: &UnmanagedSkill,
    current: &AppType,
) -> (SkillApps, Vec<AppType>) {
    let mut apps = SkillApps::default();
    let mut enabled_scopes = Vec::new();
    for found_app in AppType::all() {
        if entry
            .found_in
            .iter()
            .any(|found| found == found_app.as_str())
        {
            apps.set_enabled_for(&found_app, true);
            if apps.is_enabled_for(&found_app) {
                enabled_scopes.push(found_app);
            }
        }
    }
    // A shared-only Skill has no AppType label. The user entered this explicit
    // scope and confirmed adoption, so materialize that scope while preserving
    // every concrete scope found on disk.
    if !apps.is_enabled_for(current) {
        apps.set_enabled_for(current, true);
        enabled_scopes.push(current.clone());
    }
    (apps, enabled_scopes)
}

pub(super) fn adopt_detected(state: &AppState, app_type: &AppType) -> Result<(), AppError> {
    let source = app_type.as_str();
    let detected = SkillService::scan_unmanaged(&state.db).map_err(adopt_failed)?;
    let mut imports = Vec::new();
    let mut expected = Vec::new();

    for entry in detected
        .into_iter()
        .filter(|entry| visible_in_scope(entry, source))
    {
        let (apps, enabled_scopes) = adoption_scopes(&entry, app_type);

        expected.push((entry.directory.clone(), enabled_scopes));
        imports.push(ImportSkillSelection {
            directory: entry.directory,
            apps,
        });
    }

    if imports.is_empty() {
        return Ok(());
    }

    SkillService::import_from_apps(&state.db, imports).map_err(adopt_failed)?;
    materialize_adopted_scopes(state, &expected)?;
    let managed = SkillService::get_all_installed(&state.db).map_err(adopt_failed)?;
    for (directory, scopes) in expected {
        let imported = managed
            .iter()
            .find(|skill| skill.directory == directory)
            .ok_or_else(|| adopt_failed(format!("Skill {directory} was absent after import")))?;
        if scopes
            .iter()
            .any(|scope| !imported.apps.is_enabled_for(scope))
        {
            return Err(adopt_failed(format!(
                "Skill {directory} did not retain every detected tool scope"
            )));
        }
    }

    Ok(())
}

/// Upstream `import_from_apps` copies a detected Skill into the SSOT and
/// records the chosen flags, but never writes into a tool directory. A Skill
/// found only in the shared `~/.agents/skills` root would therefore show as
/// enabled for this tool while `~/.claude/skills/<name>` does not exist. Place
/// it wherever the flags say it is, exactly as a later toggle would; a scope
/// that cannot be placed has its flag cleared again so the inventory never
/// claims a copy that is not on disk.
fn materialize_adopted_scopes(
    state: &AppState,
    expected: &[(String, Vec<AppType>)],
) -> Result<(), AppError> {
    let _state_guard = skill_state_write_guard();
    let managed = state.db.get_all_installed_skills().map_err(adopt_failed)?;
    for (directory, scopes) in expected {
        // A missing row is reported by the flag verification that follows.
        let Some(skill) = managed.values().find(|skill| &skill.directory == directory) else {
            continue;
        };
        for scope in scopes {
            if skill_present_in(scope, directory) {
                continue;
            }
            if let Err(error) = SkillService::sync_to_app_dir(directory, scope) {
                return Err(unplaced_scope(state, skill, scope, error));
            }
        }
    }
    Ok(())
}

fn skill_present_in(app_type: &AppType, directory: &str) -> bool {
    SkillService::get_app_skills_dir(app_type)
        .map(|root| root.join(directory).is_dir())
        .unwrap_or(false)
}

fn unplaced_scope(
    state: &AppState,
    skill: &InstalledSkill,
    scope: &AppType,
    error: anyhow::Error,
) -> AppError {
    let mut apps = skill.apps.clone();
    apps.set_enabled_for(scope, false);
    match state.db.update_skill_apps(&skill.id, &apps) {
        Ok(_) => adopt_failed(format!(
            "Skill {} could not be placed in {}: {error}",
            skill.directory,
            scope.as_str()
        )),
        Err(rollback) => adopt_failed(format!(
            "Skill {} could not be placed in {} ({error}); clearing its {} flag also failed: {rollback}",
            skill.directory,
            scope.as_str(),
            scope.as_str()
        )),
    }
}

#[cfg(test)]
mod tests {
    use super::first_existing_skill_path;
    use crate::app_config::UnmanagedSkill;
    use std::fs;
    use tempfile::tempdir;

    fn make_skill(root: &std::path::Path, directory: &str) -> std::path::PathBuf {
        let path = root.join(directory);
        fs::create_dir_all(&path).expect("create Skill directory");
        fs::write(path.join("SKILL.md"), "# Local Skill").expect("write SKILL.md");
        path
    }

    #[test]
    fn detected_path_prefers_the_current_tool_copy_without_accepting_a_renderer_path() {
        let temporary = tempdir().expect("temporary directory");
        let current_root = temporary.path().join("current");
        let agents_root = temporary.path().join("agents");
        let current = make_skill(&current_root, "local-skill");
        make_skill(&agents_root, "local-skill");
        let entry = UnmanagedSkill {
            directory: "local-skill".to_string(),
            name: "Local Skill".to_string(),
            description: None,
            found_in: vec!["claude".to_string(), "agents".to_string()],
            path: "/a/path-that-must-not-be-used".to_string(),
        };

        assert_eq!(
            first_existing_skill_path(&entry, "claude", Some(current_root), agents_root, None,),
            Some(current)
        );
    }

    #[test]
    fn detected_path_requires_the_expected_directory_and_skill_document() {
        let temporary = tempdir().expect("temporary directory");
        let current_root = temporary.path().join("current");
        fs::create_dir_all(current_root.join("local-skill")).expect("create empty directory");
        let entry = UnmanagedSkill {
            directory: "local-skill".to_string(),
            name: "Local Skill".to_string(),
            description: None,
            found_in: vec!["claude".to_string()],
            path: "/tmp/untrusted".to_string(),
        };

        assert_eq!(
            first_existing_skill_path(
                &entry,
                "claude",
                Some(current_root),
                temporary.path().join("agents"),
                None,
            ),
            None
        );
    }
}
