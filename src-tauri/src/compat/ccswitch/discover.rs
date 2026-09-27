//! What the Discover section needs from the inherited MCP and Skill engines:
//! what the user already has, which transports each app can take, and where
//! a skills.sh Skill lives inside its repository.
//!
//! Server specs and Skill paths stay here. The layers above receive only the
//! few fields they compare, never a configuration payload.

use tauri::Manager;

use crate::app_config::AppType;
use crate::compat::ccswitch::extension::detail;
use crate::compat::ccswitch::extension::mcp::scan_live;
use crate::compat::ccswitch::provider::app_type_for;
use crate::domain::{
    AppError, DesktopAppId, DiscoverTransport, ErrorCode, ExtensionScope, SkillCatalogItem,
    SkillSource,
};
use crate::services::skill::{
    parse_agents_lock, DiscoverableSkill, SkillRepo, SkillRepoDelivery, SkillService,
};
use crate::services::McpService;
use crate::store::AppState;

/// One MCP connection the user has, managed or found in an app's own file,
/// reduced to what identifies what it runs.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct McpHolding {
    pub name: String,
    pub url: Option<String>,
    pub command: Option<String>,
    pub args: Vec<String>,
}

/// One Skill the user has, with the GitHub repository it came from when that
/// is known.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SkillHolding {
    pub name: String,
    pub directory: String,
    pub repo: Option<(String, String)>,
}

/// The apps whose MCP file can be scanned for connections the user added
/// outside the product.
const SCANNED_APPS: [AppType; 7] = [
    AppType::Claude,
    AppType::Codex,
    AppType::Gemini,
    AppType::GrokBuild,
    AppType::OpenCode,
    AppType::Hermes,
    AppType::ClaudeDesktop,
];

fn state(app_handle: &tauri::AppHandle) -> Result<AppState, AppError> {
    app_handle
        .try_state::<AppState>()
        .map(|state| state.inner().clone())
        .ok_or_else(|| {
            AppError::new(ErrorCode::Internal, "error.extension.storeUnavailable")
                .with_remediation("error.remediation.retryOrViewDetails")
        })
}

fn holding_from_spec(name: &str, spec: &serde_json::Value) -> McpHolding {
    let text = |key: &str| {
        spec.get(key)
            .and_then(serde_json::Value::as_str)
            .map(str::to_string)
    };
    let args = spec
        .get("args")
        .and_then(serde_json::Value::as_array)
        .map(|args| {
            args.iter()
                .filter_map(serde_json::Value::as_str)
                .map(str::to_string)
                .collect()
        })
        .unwrap_or_default();
    // Gemini's streaming form keeps its address under `httpUrl`.
    McpHolding {
        name: name.to_string(),
        url: text("url").or_else(|| text("httpUrl")),
        command: text("command"),
        args,
    }
}

/// Every MCP connection the user has: the managed rows, then whatever each
/// app's own file holds. A file that cannot be read is skipped; it only
/// means a card may offer Add for something already there.
pub fn mcp_holdings(app_handle: &tauri::AppHandle) -> Result<Vec<McpHolding>, AppError> {
    let state = state(app_handle)?;
    let managed = McpService::get_all_servers(&state).map_err(|error| {
        AppError::new(ErrorCode::UpstreamError, "error.extension.listFailed")
            .with_technical(detail(error))
            .with_remediation("error.remediation.retryOrViewDetails")
    })?;
    let mut holdings = managed
        .values()
        .map(|server| holding_from_spec(&server.name, &server.server))
        .collect::<Vec<_>>();
    for app_type in SCANNED_APPS {
        match scan_live(&app_type) {
            Ok(found) => holdings.extend(
                found
                    .iter()
                    .map(|server| holding_from_spec(&server.name, &server.server)),
            ),
            Err(error) => log::debug!(
                "discover: could not read the {} MCP file: {}",
                app_type.as_str(),
                detail(error)
            ),
        }
    }
    Ok(holdings)
}

/// Every Skill the user has. A found Skill's repository comes from the
/// `~/.agents/.skill-lock.json` that the skills CLI writes; without it the
/// Skill has no known source and never counts as a Discover item.
pub fn skill_holdings(app_handle: &tauri::AppHandle) -> Result<Vec<SkillHolding>, AppError> {
    let state = state(app_handle)?;
    let listed = |error: anyhow::Error| {
        AppError::new(ErrorCode::UpstreamError, "error.extension.listFailed")
            .with_technical(detail(error))
            .with_remediation("error.remediation.retryOrViewDetails")
    };
    let mut holdings = SkillService::get_all_installed(&state.db)
        .map_err(listed)?
        .into_iter()
        .map(|skill| SkillHolding {
            name: skill.name,
            directory: skill.directory,
            repo: skill.repo_owner.zip(skill.repo_name),
        })
        .collect::<Vec<_>>();
    match SkillService::scan_unmanaged(&state.db) {
        Ok(found) => {
            let lock = parse_agents_lock();
            holdings.extend(found.into_iter().map(|skill| {
                SkillHolding {
                    repo: lock
                        .get(&skill.directory)
                        .map(|info| (info.owner.clone(), info.repo.clone())),
                    name: skill.name,
                    directory: skill.directory,
                }
            }));
        }
        Err(error) => log::debug!("discover: could not scan found Skills: {}", detail(error)),
    }
    Ok(holdings)
}

/// Which transports an app's MCP file can express. Claude Desktop's file
/// runs local commands only; remote servers are added in its own Connectors.
/// Codex, Grok Build and Hermes reach a URL over streamable HTTP but not SSE.
pub fn mcp_transports(scope: ExtensionScope) -> Vec<DiscoverTransport> {
    use DiscoverTransport::{Http, Sse, Stdio};
    let app_type = match scope {
        ExtensionScope::Tool { id } => app_type_for(id),
        ExtensionScope::DesktopApp {
            id: DesktopAppId::ClaudeDesktop,
        } => AppType::ClaudeDesktop,
        ExtensionScope::DesktopApp { .. } => return Vec::new(),
    };
    match app_type {
        AppType::Claude | AppType::Gemini | AppType::OpenCode => vec![Stdio, Http, Sse],
        AppType::Codex | AppType::GrokBuild | AppType::Hermes => vec![Stdio, Http],
        AppType::ClaudeDesktop => vec![Stdio],
        _ => Vec::new(),
    }
}

/// Which of a repository's Skills is skills.sh's `skill_id`: the one whose
/// SKILL.md is named that, else the one in a folder of that name.
pub(crate) fn pick<'a>(
    skills: &'a [DiscoverableSkill],
    skill_id: &str,
) -> Option<&'a DiscoverableSkill> {
    skills
        .iter()
        .find(|skill| skill.name == skill_id)
        .or_else(|| {
            skills
                .iter()
                .find(|skill| skill.name.eq_ignore_ascii_case(skill_id))
        })
        .or_else(|| {
            skills.iter().find(|skill| {
                skill
                    .directory
                    .rsplit('/')
                    .next()
                    .is_some_and(|folder| folder.eq_ignore_ascii_case(skill_id))
            })
        })
}

fn download_failed(error: impl std::fmt::Display) -> AppError {
    AppError::new(ErrorCode::NetworkError, "error.skill.downloadFailed")
        .with_technical(detail(error))
        .with_remediation("error.remediation.checkInternetConnection")
}

/// Finds skills.sh's `skill_id` inside `owner/repository`, by downloading the
/// repository the way a catalog refresh does (GitHub first, the verified
/// jsDelivr snapshot after a transport failure, ADR-0022) and reading every
/// SKILL.md in it. The answer is an ordinary catalog item, installed through
/// the same path as one picked from a configured source.
pub async fn resolve_skill(
    owner: &str,
    repository: &str,
    skill_id: &str,
) -> Result<SkillCatalogItem, AppError> {
    let service = SkillService::new();
    let repo = SkillRepo {
        owner: owner.to_string(),
        name: repository.to_string(),
        branch: "HEAD".to_string(),
        enabled: true,
    };
    let (temp, branch, delivery) = tokio::time::timeout(
        std::time::Duration::from_secs(60),
        service.download_repo(&repo),
    )
    .await
    .map_err(|_| download_failed("the repository download timed out"))?
    .map_err(download_failed)?;
    let resolved = SkillRepo {
        branch: branch.clone(),
        ..repo
    };
    let mut skills = Vec::new();
    service
        .scan_dir_recursive(temp.path(), temp.path(), &resolved, &mut skills)
        .map_err(download_failed)?;
    let skill = pick(&skills, skill_id).ok_or_else(|| {
        AppError::new(ErrorCode::ExtensionNotFound, "error.discover.skillMissing")
            .with_technical(format!(
                "{owner}/{repository} has {} Skills, none is {skill_id}",
                skills.len()
            ))
            .with_remediation("error.remediation.retryOrViewDetails")
    })?;
    let description = skill.description.trim().to_string();
    let item = SkillCatalogItem {
        id: skill.key.clone(),
        name: skill.name.trim().to_string(),
        description: (!description.is_empty()).then_some(description),
        source: SkillSource {
            owner: owner.to_string(),
            repository: repository.to_string(),
            branch,
            directory: skill.directory.clone(),
        },
        installed: false,
        mirror_used: delivery == SkillRepoDelivery::Jsdelivr,
    };
    item.validate_source()?;
    Ok(item)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn discoverable(name: &str, directory: &str) -> DiscoverableSkill {
        DiscoverableSkill {
            key: format!("owner/repo:{directory}"),
            name: name.to_string(),
            description: String::new(),
            directory: directory.to_string(),
            readme_url: None,
            repo_owner: "owner".to_string(),
            repo_name: "repo".to_string(),
            repo_branch: "main".to_string(),
            mirror_used: false,
        }
    }

    #[test]
    fn a_skill_is_found_by_its_skill_md_name_before_its_folder() {
        let skills = vec![
            discoverable("other", "skills/pdf"),
            discoverable("pdf", "skills/document-pdf"),
        ];
        assert_eq!(
            pick(&skills, "pdf").map(|skill| skill.directory.as_str()),
            Some("skills/document-pdf")
        );
    }

    #[test]
    fn a_skill_is_found_by_its_folder_when_no_name_matches() {
        let skills = vec![discoverable("Frontend Design", "skills/frontend-design")];
        assert_eq!(
            pick(&skills, "frontend-design").map(|skill| skill.name.as_str()),
            Some("Frontend Design")
        );
        assert!(pick(&skills, "backend").is_none());
    }

    #[test]
    fn holdings_read_the_url_command_and_arguments_of_a_spec() {
        let remote = holding_from_spec(
            "docs",
            &serde_json::json!({"type": "http", "url": "https://a.example/mcp"}),
        );
        assert_eq!(remote.url.as_deref(), Some("https://a.example/mcp"));
        let gemini = holding_from_spec(
            "docs",
            &serde_json::json!({"httpUrl": "https://b.example/mcp"}),
        );
        assert_eq!(gemini.url.as_deref(), Some("https://b.example/mcp"));
        let local = holding_from_spec(
            "fs",
            &serde_json::json!({"command": "npx", "args": ["-y", "pkg"]}),
        );
        assert_eq!(local.command.as_deref(), Some("npx"));
        assert_eq!(local.args, vec!["-y".to_string(), "pkg".to_string()]);
    }

    #[test]
    fn every_mcp_app_can_run_a_local_command_and_only_some_take_sse() {
        use crate::domain::ToolId;
        assert_eq!(
            mcp_transports(ExtensionScope::desktop_app(DesktopAppId::ClaudeDesktop)),
            vec![DiscoverTransport::Stdio]
        );
        assert!(
            !mcp_transports(ExtensionScope::tool(ToolId::Codex)).contains(&DiscoverTransport::Sse)
        );
        assert!(mcp_transports(ExtensionScope::tool(ToolId::ClaudeCode))
            .contains(&DiscoverTransport::Sse));
    }
}
