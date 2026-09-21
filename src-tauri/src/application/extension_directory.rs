//! Use-case orchestration for extensions (spec §9 / §91).
//!
//! This layer does two things only: decide whether an action is allowed based on
//! `ToolCapabilities`, then hand the request to the compatibility layer. It **must not** contain
//! any upstream type — boundary rule R2 blocks that in CI.

use crate::compat::ccswitch::extension::ExtensionStore;
use crate::compat::ccswitch::tools::capabilities_for;
use crate::domain::{
    AppError, DesktopAppId, DetectedSkillResourceAction, DetectedSkillResourceOpenOutcome,
    ErrorCode, Extension, ExtensionKind, ExtensionManagement, ExtensionScope,
    LocalExtensionInventory, LocalExtensionScope, LocalExtensionScopeStatus, ToolCapabilities,
    ToolId,
};
use crate::platform::Platform;
use tauri_plugin_opener::OpenerExt;

const LOCAL_INVENTORY_ITEM_LIMIT: usize = 512;

fn local_inventory_scopes() -> Vec<(ToolId, ExtensionKind)> {
    ToolId::ALL
        .into_iter()
        .flat_map(|tool| {
            let capabilities = capabilities_for(tool);
            [ExtensionKind::Skill, ExtensionKind::Mcp]
                .into_iter()
                .filter(move |kind| supports(*kind, &capabilities))
                .map(move |kind| (tool, kind))
        })
        .collect()
}

fn collect_local_inventory<F>(mut list: F) -> LocalExtensionInventory
where
    F: FnMut(ToolId, ExtensionKind) -> Result<Vec<Extension>, AppError>,
{
    let mut items = Vec::new();
    let mut scopes = Vec::new();
    let mut truncated = false;

    for (tool, kind) in local_inventory_scopes() {
        let status = match list(tool, kind) {
            Ok(entries) => {
                for entry in entries.into_iter().filter(|entry| {
                    entry.management == ExtensionManagement::Detected
                        && entry.scope == ExtensionScope::tool(tool)
                        && entry.kind == kind
                }) {
                    if items.len() < LOCAL_INVENTORY_ITEM_LIMIT {
                        items.push(entry);
                    } else {
                        truncated = true;
                    }
                }
                LocalExtensionScopeStatus::Ready
            }
            Err(_) => LocalExtensionScopeStatus::Unavailable,
        };
        scopes.push(LocalExtensionScope { tool, kind, status });
    }

    LocalExtensionInventory {
        items,
        scopes,
        truncated,
    }
}

/// Which slot of `ToolCapabilities` each of the three kinds reads. This is the only per-kind
/// branching knowledge in the whole phase, and it is a static table pinned by an exhaustive test
/// (a shape AI_RULES rule 8 allows).
/// The three fields already existed in Phase 1 and were filled in per tool; this phase is their
/// first consumer.
pub(crate) fn supports(kind: ExtensionKind, capabilities: &ToolCapabilities) -> bool {
    match kind {
        ExtensionKind::Skill => capabilities.can_manage_skills,
        ExtensionKind::Mcp => capabilities.can_manage_mcp,
        ExtensionKind::Prompt => capabilities.can_manage_prompts,
    }
}

fn supports_scope(scope: ExtensionScope, kind: ExtensionKind) -> bool {
    match scope {
        ExtensionScope::Tool { id } => supports(kind, &capabilities_for(id)),
        ExtensionScope::DesktopApp {
            id: DesktopAppId::ClaudeDesktop,
        } => {
            kind == ExtensionKind::Mcp
                && matches!(Platform::current(), Platform::MacOs | Platform::Windows)
        }
        ExtensionScope::DesktopApp { .. } => false,
    }
}

fn unsupported(scope: ExtensionScope, kind: ExtensionKind) -> AppError {
    AppError::new(
        ErrorCode::ExtensionNotFound,
        "error.extension.unsupportedScope",
    )
    .with_technical(format!(
        "{} cannot manage {} extensions",
        scope.stable_key(),
        kind.as_str()
    ))
}

fn gate(
    app_handle: &tauri::AppHandle,
    scope: ExtensionScope,
    kind: ExtensionKind,
) -> Result<ExtensionStore, AppError> {
    if !supports_scope(scope, kind) {
        return Err(unsupported(scope, kind));
    }
    ExtensionStore::open(app_handle)
}

pub struct ExtensionDirectory;

impl ExtensionDirectory {
    pub fn reveal_location(
        app_handle: &tauri::AppHandle,
        scope: ExtensionScope,
        kind: ExtensionKind,
    ) -> Result<(), AppError> {
        if !supports_scope(scope, kind) {
            return Err(unsupported(scope, kind));
        }
        let path = crate::compat::ccswitch::extension::location_path(scope, kind)?;
        super::reveal::open_in_file_manager(app_handle, &path).map_err(resource_open_error)
    }

    /// Scan every capability-backed local Skills/MCP scope and return only the
    /// safe detected projections. One malformed tool config becomes an
    /// unavailable scope rather than hiding successful discoveries elsewhere.
    pub fn local_inventory(
        app_handle: &tauri::AppHandle,
    ) -> Result<LocalExtensionInventory, AppError> {
        let store = ExtensionStore::open(app_handle)?;
        Ok(collect_local_inventory(|tool, kind| store.list(tool, kind)))
    }

    pub fn list(
        app_handle: &tauri::AppHandle,
        scope: ExtensionScope,
        kind: ExtensionKind,
    ) -> Result<Vec<Extension>, AppError> {
        gate(app_handle, scope, kind)?.list_scope(scope, kind)
    }

    pub fn set_enabled(
        app_handle: &tauri::AppHandle,
        scope: ExtensionScope,
        kind: ExtensionKind,
        id: &str,
        enabled: bool,
    ) -> Result<Vec<Extension>, AppError> {
        gate(app_handle, scope, kind)?.set_enabled_scope(scope, kind, id, enabled)
    }

    pub fn open_detected_skill_resource(
        app_handle: &tauri::AppHandle,
        scope: ExtensionScope,
        id: &str,
        action: DetectedSkillResourceAction,
    ) -> Result<DetectedSkillResourceOpenOutcome, AppError> {
        let directory =
            gate(app_handle, scope, ExtensionKind::Skill)?.detected_skill_path(scope, id)?;
        let (target, outcome) = match action {
            DetectedSkillResourceAction::Browse => {
                if !directory.is_dir() {
                    return Err(resource_open_error(
                        "detected Skill directory disappeared before opening",
                    ));
                }
                (directory, DetectedSkillResourceOpenOutcome::FolderOpened)
            }
            DetectedSkillResourceAction::Edit => {
                let document = directory.join("SKILL.md");
                if !document.is_file() {
                    return Err(resource_open_error(
                        "detected Skill document disappeared before opening",
                    ));
                }
                (document, DetectedSkillResourceOpenOutcome::EditorOpened)
            }
        };

        app_handle
            .opener()
            .open_path(target.to_string_lossy().to_string(), None::<String>)
            .map_err(|error| {
                resource_open_error(format!("system opener failed for detected Skill: {error}"))
            })?;
        Ok(outcome)
    }

    pub fn adopt_detected(
        app_handle: &tauri::AppHandle,
        scope: ExtensionScope,
        kind: ExtensionKind,
    ) -> Result<Vec<Extension>, AppError> {
        gate(app_handle, scope, kind)?.adopt_detected_scope(scope, kind)
    }

    /// Copy one detected Skill into another tool and return that tool's
    /// refreshed inventory.
    ///
    /// The target is checked before anything else: `app_type_for` panics on the
    /// tools that have no upstream `AppType`, so an unsupported target must be
    /// turned away here rather than reaching the compatibility layer.
    pub fn copy_detected_skill(
        app_handle: &tauri::AppHandle,
        source: ExtensionScope,
        target: ToolId,
        id: &str,
    ) -> Result<Vec<Extension>, AppError> {
        let target_scope = ExtensionScope::tool(target);
        if !supports_scope(target_scope, ExtensionKind::Skill) {
            return Err(unsupported(target_scope, ExtensionKind::Skill));
        }
        gate(app_handle, source, ExtensionKind::Skill)?.copy_detected_skill(source, target, id)
    }
}

fn resource_open_error(technical: impl Into<String>) -> AppError {
    AppError::new(
        ErrorCode::LaunchFailed,
        "error.extension.resourceOpenFailed",
    )
    .with_technical(technical)
    .with_remediation("error.remediation.checkLocalFileAccess")
}

#[cfg(test)]
mod tests {
    use super::{collect_local_inventory, local_inventory_scopes, supports, unsupported};
    use crate::compat::ccswitch::tools::capabilities_for;
    use crate::domain::{
        AppError, ErrorCode, Extension, ExtensionKind, ExtensionManagement, ExtensionScope,
        LocalExtensionScopeStatus, ToolCapabilities, ToolId,
    };

    #[test]
    fn each_kind_reads_its_own_capability_field_and_no_other() {
        // With only one slot enabled, only the matching kind passes. What this pins down is that
        // "the gate reads the capability", which still holds when tools or kinds are added later.
        let cases = [
            (
                ExtensionKind::Skill,
                ToolCapabilities {
                    can_manage_skills: true,
                    ..ToolCapabilities::default()
                },
            ),
            (
                ExtensionKind::Mcp,
                ToolCapabilities {
                    can_manage_mcp: true,
                    ..ToolCapabilities::default()
                },
            ),
            (
                ExtensionKind::Prompt,
                ToolCapabilities {
                    can_manage_prompts: true,
                    ..ToolCapabilities::default()
                },
            ),
        ];
        for (allowed, capabilities) in cases {
            for kind in ExtensionKind::ALL {
                assert_eq!(
                    supports(kind, &capabilities),
                    kind == allowed,
                    "{kind:?} read the wrong capability field"
                );
            }
        }
    }

    #[test]
    fn nothing_is_allowed_when_every_capability_is_off() {
        let none = ToolCapabilities::default();
        for kind in ExtensionKind::ALL {
            assert!(!supports(kind, &none));
        }
    }

    #[test]
    fn the_shipped_capability_table_matches_what_the_page_will_offer() {
        // Keep the product gates aligned with the inherited adapters instead
        // of silently exposing only the first application implemented by the
        // renderer. Seven tools have native Skills, six have native MCP, and
        // every AppType-backed CLI has a managed instruction-file path.
        for tool in [
            ToolId::ClaudeCode,
            ToolId::Codex,
            ToolId::OpenCode,
            ToolId::GeminiCli,
            ToolId::GrokBuild,
            ToolId::Hermes,
            ToolId::Pi,
        ] {
            let capabilities = capabilities_for(tool);
            assert!(supports(ExtensionKind::Skill, &capabilities), "{tool:?}");
            assert!(supports(ExtensionKind::Prompt, &capabilities), "{tool:?}");
        }
        for tool in [
            ToolId::ClaudeCode,
            ToolId::Codex,
            ToolId::OpenCode,
            ToolId::GeminiCli,
            ToolId::GrokBuild,
            ToolId::Hermes,
        ] {
            assert!(
                supports(ExtensionKind::Mcp, &capabilities_for(tool)),
                "{tool:?}"
            );
        }

        let openclaw = capabilities_for(ToolId::OpenClaw);
        assert!(supports(ExtensionKind::Prompt, &openclaw));
        assert!(!supports(ExtensionKind::Skill, &openclaw));
        assert!(!supports(ExtensionKind::Mcp, &openclaw));
    }

    #[test]
    fn refusing_an_unsupported_tool_says_so_in_a_translatable_way() {
        let error = unsupported(ExtensionScope::tool(ToolId::OpenClaw), ExtensionKind::Skill);
        assert_eq!(error.code, ErrorCode::ExtensionNotFound);
        assert_eq!(error.message_key, "error.extension.unsupportedScope");
        assert_eq!(
            error.technical_message.as_deref(),
            Some("tool:openclaw cannot manage skill extensions")
        );
    }

    #[test]
    fn local_inventory_scans_every_capability_scope_but_never_prompts() {
        let scopes = local_inventory_scopes();
        assert_eq!(scopes.len(), 13);
        assert!(scopes.contains(&(ToolId::ClaudeCode, ExtensionKind::Skill)));
        assert!(scopes.contains(&(ToolId::Codex, ExtensionKind::Mcp)));
        assert!(scopes.contains(&(ToolId::Pi, ExtensionKind::Skill)));
        assert!(!scopes
            .iter()
            .any(|(_, kind)| *kind == ExtensionKind::Prompt));
        assert!(!scopes.contains(&(ToolId::OpenClaw, ExtensionKind::Skill)));
        assert!(!scopes.contains(&(ToolId::OpenClaw, ExtensionKind::Mcp)));
    }

    #[test]
    fn local_inventory_keeps_good_scopes_and_marks_failed_ones_without_payloads() {
        let detected = Extension {
            kind: ExtensionKind::Skill,
            id: "local-release".to_string(),
            scope: ExtensionScope::tool(ToolId::ClaudeCode),
            name: "Local release".to_string(),
            description: None,
            management: ExtensionManagement::Detected,
            enabled: true,
            can_disable: false,
        };
        let managed = Extension {
            management: ExtensionManagement::Managed,
            ..detected.clone()
        };
        let inventory = collect_local_inventory(|tool, kind| {
            if (tool, kind) == (ToolId::Codex, ExtensionKind::Mcp) {
                return Err(AppError::new(
                    ErrorCode::UpstreamError,
                    "error.extension.listFailed",
                ));
            }
            if (tool, kind) == (ToolId::ClaudeCode, ExtensionKind::Skill) {
                return Ok(vec![managed.clone(), detected.clone()]);
            }
            Ok(Vec::new())
        });

        assert_eq!(inventory.items, vec![detected]);
        assert!(!inventory.truncated);
        assert_eq!(inventory.scopes.len(), 13);
        assert_eq!(
            inventory
                .scopes
                .iter()
                .find(|scope| { scope.tool == ToolId::Codex && scope.kind == ExtensionKind::Mcp })
                .map(|scope| scope.status),
            Some(LocalExtensionScopeStatus::Unavailable)
        );
        assert!(inventory.scopes.iter().all(|scope| {
            (scope.tool == ToolId::Codex && scope.kind == ExtensionKind::Mcp)
                || scope.status == LocalExtensionScopeStatus::Ready
        }));
    }
}
