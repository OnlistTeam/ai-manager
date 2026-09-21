use std::path::{Path, PathBuf};

use crate::compat::ccswitch::tools::tool_id_to_app_type;
use crate::domain::{
    AppError, ErrorCode, ProviderRuntimeContext, ProviderRuntimeResource,
    ProviderRuntimeResourceAction, ProviderRuntimeResourceKind, ProviderRuntimeResourceScope,
    ShellVariableLocation, ShellVariableUpdate, ShellVariableWritten, ToolId,
};

mod effective;
mod environment;
mod shell_edit;
mod shell_files;
mod storage;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum RuntimeResourceTarget {
    File,
    Directory,
}

#[derive(Debug, Clone)]
pub(crate) struct ResolvedRuntimeResource {
    pub resource: ProviderRuntimeResource,
    pub path: PathBuf,
    pub target: RuntimeResourceTarget,
}

/// The terminal environment as this tool sees it. Both the runtime context and
/// the model probe start here, so they always agree on what is in force.
fn tool_environment(tool: ToolId) -> Result<environment::ToolEnvironment, AppError> {
    let app_type_name = tool_id_to_app_type(tool).ok_or_else(|| {
        AppError::new(
            ErrorCode::ProviderNotFound,
            "error.provider.unsupportedTool",
        )
        .with_technical(format!(
            "{} has no CC Switch provider AppType",
            tool.as_str()
        ))
    })?;
    environment::ToolEnvironment::detect(tool, app_type_name).map_err(|error| {
        AppError::new(
            ErrorCode::ConfigParseFailed,
            "error.provider.runtimeContextFailed",
        )
        .with_technical(error)
        .with_remediation("error.remediation.retryOrViewDetails")
    })
}

pub fn runtime_context(
    app_handle: &tauri::AppHandle,
    tool: ToolId,
) -> Result<ProviderRuntimeContext, AppError> {
    let environment = tool_environment(tool)?;
    let (providers, current) =
        crate::compat::ccswitch::provider::ProviderStore::open(app_handle)?.raw_inventory(tool)?;
    let effective_connection =
        effective::resolve_effective_connection(tool, &environment, &providers, &current);
    let mut resources = resolved_resources(tool);
    let storage = storage::measure_runtime_storage(tool, &mut resources);
    Ok(ProviderRuntimeContext {
        tool,
        live_config_paths: live_paths(tool)
            .into_iter()
            .map(|path| display_path(&path))
            .collect(),
        resources: resources
            .into_iter()
            .map(|resolved| resolved.resource)
            .collect(),
        storage,
        effective_connection,
    })
}

/// Address and credential of the connection the tool will actually use, for the
/// model probe (ADR-0041).
///
/// This is the path behind the "test" action on a connection that is not a
/// saved service — one the user set up in a shell profile or wrote into the
/// tool's own configuration file. The values are resolved here, in the backend,
/// exactly as `runtime_context` resolves them for display; the renderer sends
/// neither and receives neither.
///
/// `Ok(None)` means there is nothing to test: no address, or a credential this
/// resolver cannot replay (an OAuth login rather than a key).
pub(crate) fn effective_probe_target(tool: ToolId) -> Result<Option<(String, String)>, AppError> {
    Ok(effective::resolve_probe_target(
        tool,
        &tool_environment(tool)?,
    ))
}

/// Where the tool's connection variables are written down, and which of them
/// this product can safely change (ADR-0042).
///
/// Only variables the tool itself reads are searched, so this can never be
/// turned into a general "read any variable out of my shell" call. A variable
/// the shell exports but no start-up file explains is absent from the result:
/// something sourced it in a way this product does not model, and a line it
/// cannot point at is a line it must not offer to edit.
pub(crate) fn shell_variable_locations(tool: ToolId) -> Vec<ShellVariableLocation> {
    let home = crate::config::get_home_dir();
    let environment = tool_environment(tool).ok();
    environment::connection_variables(tool)
        .iter()
        .filter_map(|variable| {
            // The login shell decides which of several assignments is in force,
            // so a value it does not report is not attributed to any line.
            let value = environment
                .as_ref()
                .and_then(|environment| environment.lookup(variable))?
                .value;
            let site = shell_files::responsible_site(&home, variable, &value)?;
            Some(ShellVariableLocation {
                variable: site.variable,
                path: display_source(&site.path.to_string_lossy()),
                line: site.line,
                value: site.value,
                editable: site.rewritable,
            })
        })
        .collect()
}

/// Replaces one value on one line of a start-up file.
pub(crate) fn write_shell_variable(
    tool: ToolId,
    update: &ShellVariableUpdate,
) -> Result<ShellVariableWritten, AppError> {
    // The variable has to be one this tool actually reads. Without this, the
    // renderer could name any variable in the user's profile.
    if !environment::connection_variables(tool)
        .iter()
        .any(|candidate| candidate.eq_ignore_ascii_case(&update.variable))
    {
        return Err(AppError::new(
            ErrorCode::ProviderNotFound,
            "error.shellVariable.notAConnectionVariable",
        )
        .with_technical(format!(
            "{} does not read a variable by that name",
            tool.as_str()
        ))
        .with_remediation("error.remediation.checkServiceSettings"));
    }
    let path = shell_edit::write_variable(
        &crate::config::get_home_dir(),
        &crate::infrastructure::paths::product_data_dir()
            .join("backups")
            .join("shell"),
        shell_edit::ShellVariableEdit {
            variable: &update.variable,
            line: update.line,
            expected_value: &update.expected_value,
            new_value: &update.new_value,
        },
    )?;
    Ok(ShellVariableWritten {
        path: display_source(&path.to_string_lossy()),
        line: update.line,
    })
}

fn live_paths(tool: ToolId) -> Vec<PathBuf> {
    match tool {
        ToolId::ClaudeCode => vec![crate::config::get_claude_settings_path()],
        ToolId::Codex => vec![
            crate::codex_config::get_codex_config_path(),
            crate::codex_config::get_codex_auth_path(),
        ],
        ToolId::OpenCode => vec![crate::opencode_config::get_opencode_config_path()],
        ToolId::GeminiCli => vec![
            crate::gemini_config::get_gemini_env_path(),
            crate::gemini_config::get_gemini_settings_path(),
        ],
        ToolId::GrokBuild => vec![crate::grok_config::get_grok_config_path()],
        ToolId::OpenClaw => vec![crate::openclaw_config::get_openclaw_config_path()],
        ToolId::Hermes => vec![crate::hermes_config::get_hermes_config_path()],
        ToolId::Pi => [
            crate::pi_config::get_pi_models_path(),
            crate::pi_config::get_pi_settings_path(),
        ]
        .into_iter()
        .flatten()
        .collect(),
        ToolId::KimiCode | ToolId::DeepSeekDsh => Vec::new(),
    }
}

pub(crate) fn resolve_resource(
    tool: ToolId,
    resource_id: &str,
) -> Result<ResolvedRuntimeResource, AppError> {
    resolved_resources(tool)
        .into_iter()
        .find(|resolved| resolved.resource.id == resource_id)
        .ok_or_else(|| {
            AppError::new(
                ErrorCode::ProviderNotFound,
                "error.provider.runtimeResourceNotFound",
            )
            .with_technical(format!(
                "runtime resource {resource_id:?} is not registered for {}",
                tool.as_str()
            ))
            .with_remediation("error.remediation.retryOrViewDetails")
        })
}

fn resolved_resources(tool: ToolId) -> Vec<ResolvedRuntimeResource> {
    let mut resources = Vec::new();

    for (index, path) in live_paths(tool).into_iter().enumerate() {
        // If the file already exists, hand it straight to the system default application. If it
        // does not, all we can open is the folder it will live in: Edit would create the missing
        // file first, and an empty settings.json is not valid JSON, so the tool would fail on its
        // next launch.
        let action = if path.is_file() {
            ProviderRuntimeResourceAction::Edit
        } else {
            ProviderRuntimeResourceAction::Browse
        };
        push_resource(
            &mut resources,
            format!("live-config-{index}"),
            ProviderRuntimeResourceKind::Configuration,
            ProviderRuntimeResourceScope::Global,
            action,
            path,
            RuntimeResourceTarget::File,
        );
    }

    if let Some(app_type) = app_type(tool) {
        if let Ok(path) = crate::prompt_files::prompt_file_path(&app_type) {
            push_resource(
                &mut resources,
                "global-instructions",
                ProviderRuntimeResourceKind::Instructions,
                ProviderRuntimeResourceScope::Global,
                ProviderRuntimeResourceAction::Edit,
                path,
                RuntimeResourceTarget::File,
            );
        }
    }

    match tool {
        ToolId::Hermes => {
            let memories = crate::hermes_config::get_hermes_dir().join("memories");
            push_resource(
                &mut resources,
                "memory",
                ProviderRuntimeResourceKind::Memory,
                ProviderRuntimeResourceScope::Global,
                ProviderRuntimeResourceAction::Edit,
                memories.join("MEMORY.md"),
                RuntimeResourceTarget::File,
            );
            push_resource(
                &mut resources,
                "user-profile",
                ProviderRuntimeResourceKind::UserProfile,
                ProviderRuntimeResourceScope::Global,
                ProviderRuntimeResourceAction::Edit,
                memories.join("USER.md"),
                RuntimeResourceTarget::File,
            );
        }
        ToolId::OpenClaw => push_resource(
            &mut resources,
            "project-memory",
            ProviderRuntimeResourceKind::Memory,
            ProviderRuntimeResourceScope::Project,
            ProviderRuntimeResourceAction::Browse,
            crate::openclaw_config::get_openclaw_dir()
                .join("workspace")
                .join("memory"),
            RuntimeResourceTarget::Directory,
        ),
        ToolId::ClaudeCode
        | ToolId::Codex
        | ToolId::OpenCode
        | ToolId::GeminiCli
        | ToolId::GrokBuild
        | ToolId::Pi
        | ToolId::KimiCode
        | ToolId::DeepSeekDsh => {}
    }

    for (index, path) in crate::compat::ccswitch::tool_paths::cache_paths(tool)
        .into_iter()
        .enumerate()
    {
        push_resource(
            &mut resources,
            format!("session-data-{index}"),
            ProviderRuntimeResourceKind::SessionData,
            ProviderRuntimeResourceScope::Project,
            ProviderRuntimeResourceAction::Browse,
            path,
            RuntimeResourceTarget::Directory,
        );
    }

    resources
}

fn push_resource(
    resources: &mut Vec<ResolvedRuntimeResource>,
    id: impl Into<String>,
    kind: ProviderRuntimeResourceKind,
    scope: ProviderRuntimeResourceScope,
    action: ProviderRuntimeResourceAction,
    path: PathBuf,
    target: RuntimeResourceTarget,
) {
    let exists = path.exists();
    resources.push(ResolvedRuntimeResource {
        resource: ProviderRuntimeResource {
            id: id.into(),
            kind,
            scope,
            path: display_path(&path),
            exists,
            action,
            size_bytes: None,
            measurement_limited: false,
        },
        path,
        target,
    });
}

fn app_type(tool: ToolId) -> Option<crate::app_config::AppType> {
    use crate::app_config::AppType;
    match tool {
        ToolId::ClaudeCode => Some(AppType::Claude),
        ToolId::Codex => Some(AppType::Codex),
        ToolId::OpenCode => Some(AppType::OpenCode),
        ToolId::GeminiCli => Some(AppType::Gemini),
        ToolId::GrokBuild => Some(AppType::GrokBuild),
        ToolId::OpenClaw => Some(AppType::OpenClaw),
        ToolId::Hermes => Some(AppType::Hermes),
        ToolId::Pi => Some(AppType::Pi),
        ToolId::KimiCode | ToolId::DeepSeekDsh => None,
    }
}

pub(super) fn safe_endpoint(raw: &str) -> Option<String> {
    let mut parsed = url::Url::parse(raw.trim()).ok()?;
    if !matches!(parsed.scheme(), "http" | "https")
        || parsed.host_str().is_none()
        || !parsed.username().is_empty()
        || parsed.password().is_some()
    {
        return None;
    }
    parsed.set_query(None);
    parsed.set_fragment(None);
    Some(parsed.to_string())
}

pub(super) fn display_source(source: &str) -> String {
    if source == "Process Environment" {
        return "process-environment".to_string();
    }
    let home = crate::config::get_home_dir();
    let home_text = home.to_string_lossy();
    source
        .strip_prefix(home_text.as_ref())
        .map(|suffix| format!("~{suffix}"))
        .unwrap_or_else(|| source.to_string())
}

fn display_path(path: &Path) -> String {
    let home = crate::config::get_home_dir();
    if let Ok(relative) = path.strip_prefix(&home) {
        return format!("~/{}", relative.to_string_lossy());
    }
    path.file_name()
        .map(|name| format!("…/{}", name.to_string_lossy()))
        .unwrap_or_else(|| "custom-location".to_string())
}

#[cfg(test)]
mod tests {
    use super::{display_source, resolved_resources, safe_endpoint, RuntimeResourceTarget};
    use crate::domain::{
        ProviderRuntimeResourceAction, ProviderRuntimeResourceKind, ProviderRuntimeResourceScope,
        ToolId,
    };

    /// Point the tool paths at an empty temporary home so the assertions never depend on
    /// the developer's real `~/.claude` or `~/.codex` contents.
    struct TestHome {
        previous: Option<std::ffi::OsString>,
        _temp: tempfile::TempDir,
    }

    impl TestHome {
        fn set() -> Self {
            let temp = tempfile::tempdir().expect("temp home");
            let previous = std::env::var_os("AI_MANAGER_TEST_HOME");
            std::env::set_var("AI_MANAGER_TEST_HOME", temp.path());
            Self {
                previous,
                _temp: temp,
            }
        }
    }

    impl Drop for TestHome {
        fn drop(&mut self) {
            match self.previous.take() {
                Some(value) => std::env::set_var("AI_MANAGER_TEST_HOME", value),
                None => std::env::remove_var("AI_MANAGER_TEST_HOME"),
            }
        }
    }

    /// `display_path` renders `~` plus the *native* relative path, so a Windows user sees
    /// `~/.codex\AGENTS.md`. Expectations are spelled with POSIX separators and translated
    /// here rather than hardcoded.
    fn under_home(relative: &str) -> String {
        format!("~/{}", relative.replace('/', std::path::MAIN_SEPARATOR_STR))
    }

    #[test]
    fn endpoint_projection_removes_query_credentials_and_rejects_userinfo() {
        assert_eq!(
            safe_endpoint("https://api.example.com/v1?token=secret#x").as_deref(),
            Some("https://api.example.com/v1")
        );
        assert!(safe_endpoint("https://user:pass@example.com/v1").is_none());
        assert!(safe_endpoint("$CUSTOM_ENDPOINT").is_none());
    }

    #[test]
    fn process_source_uses_a_stable_non_path_label() {
        assert_eq!(display_source("Process Environment"), "process-environment");
    }

    #[test]
    #[serial_test::serial]
    fn codex_resources_expose_global_instructions_without_exposing_an_open_path_api() {
        let _home = TestHome::set();
        let resources = resolved_resources(ToolId::Codex);
        let instructions = resources
            .iter()
            .find(|item| item.resource.id == "global-instructions")
            .expect("Codex global instructions");
        assert_eq!(
            instructions.resource.kind,
            ProviderRuntimeResourceKind::Instructions
        );
        assert_eq!(
            instructions.resource.scope,
            ProviderRuntimeResourceScope::Global
        );
        assert_eq!(
            instructions.resource.action,
            ProviderRuntimeResourceAction::Edit
        );
        assert_eq!(instructions.target, RuntimeResourceTarget::File);
        assert_eq!(instructions.resource.path, under_home(".codex/AGENTS.md"));
    }

    #[test]
    #[serial_test::serial]
    fn claude_global_instructions_are_editable_while_project_storage_is_browsed() {
        let _home = TestHome::set();
        let resources = resolved_resources(ToolId::ClaudeCode);
        let instructions = resources
            .iter()
            .find(|item| item.resource.id == "global-instructions")
            .expect("Claude global instructions");
        assert_eq!(
            instructions.resource.kind,
            ProviderRuntimeResourceKind::Instructions
        );
        assert_eq!(
            instructions.resource.scope,
            ProviderRuntimeResourceScope::Global
        );
        assert_eq!(
            instructions.resource.action,
            ProviderRuntimeResourceAction::Edit
        );
        assert_eq!(instructions.target, RuntimeResourceTarget::File);
        assert_eq!(instructions.resource.path, under_home(".claude/CLAUDE.md"));

        let sessions = resources
            .iter()
            .find(|item| item.resource.kind == ProviderRuntimeResourceKind::SessionData)
            .expect("Claude project store");
        assert_eq!(
            sessions.resource.scope,
            ProviderRuntimeResourceScope::Project
        );
        assert_eq!(
            sessions.resource.action,
            ProviderRuntimeResourceAction::Browse
        );
        assert_eq!(sessions.target, RuntimeResourceTarget::Directory);
        assert_eq!(sessions.resource.path, under_home(".claude/projects"));
    }

    #[test]
    #[serial_test::serial]
    fn live_credential_and_configuration_files_only_offer_their_parent_folder() {
        let _home = TestHome::set();
        for tool in [ToolId::ClaudeCode, ToolId::Codex] {
            let resources = resolved_resources(tool);
            let configurations = resources
                .iter()
                .filter(|item| item.resource.kind == ProviderRuntimeResourceKind::Configuration)
                .collect::<Vec<_>>();
            assert!(!configurations.is_empty(), "{tool:?} live config resources");
            for configuration in configurations {
                assert_eq!(
                    configuration.resource.action,
                    ProviderRuntimeResourceAction::Browse
                );
                assert_eq!(configuration.target, RuntimeResourceTarget::File);
            }
        }
    }

    #[test]
    #[serial_test::serial]
    fn hermes_memory_files_are_registered_as_explicit_edit_targets() {
        let _home = TestHome::set();
        let resources = resolved_resources(ToolId::Hermes);
        for id in ["memory", "user-profile"] {
            let resource = resources
                .iter()
                .find(|item| item.resource.id == id)
                .expect("Hermes memory resource");
            assert_eq!(
                resource.resource.action,
                ProviderRuntimeResourceAction::Edit
            );
            assert_eq!(resource.target, RuntimeResourceTarget::File);
        }
    }
}
