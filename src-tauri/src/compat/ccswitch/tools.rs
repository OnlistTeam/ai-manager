use std::cmp::Ordering;

use crate::commands::misc::{
    compare_semver, get_single_tool_version_impl, probe_local_tool_version, ToolVersion,
};
use crate::compat::ccswitch::install_probe::{probe, LifecycleProbe};
use crate::domain::{AppError, Tool, ToolCapabilities, ToolDiscovery, ToolId, ToolStatus};

mod capabilities;
mod latest_version;
pub use capabilities::capabilities_for;
pub(crate) use capabilities::capabilities_for_probe;

/// ADR-0004: the single mapping point between ToolId and the upstream AppType string.
///
/// Kimi Code and DeepSeek DSH currently only plug into the product's own software lifecycle;
/// CC Switch has no matching `AppType`, so they must return `None` rather than borrow another
/// tool's config format.
pub fn tool_id_to_app_type(id: ToolId) -> Option<&'static str> {
    match id {
        ToolId::ClaudeCode => Some("claude"),
        ToolId::Codex => Some("codex"),
        ToolId::OpenCode => Some("opencode"),
        ToolId::GeminiCli => Some("gemini"),
        ToolId::GrokBuild => Some("grokbuild"),
        ToolId::OpenClaw => Some("openclaw"),
        ToolId::Hermes => Some("hermes"),
        ToolId::Pi => Some("pi"),
        ToolId::KimiCode | ToolId::DeepSeekDsh => None,
    }
}

/// The upstream config identifier usually equals the real CLI name; Grok Build is the only
/// exception: the config layer calls it `grokbuild` while version detection, WSL and the
/// execution layer call the binary `grok` (ADR-0005).
pub fn tool_id_to_cli_name(id: ToolId) -> &'static str {
    match id {
        ToolId::ClaudeCode => "claude",
        ToolId::Codex => "codex",
        ToolId::OpenCode => "opencode",
        ToolId::GeminiCli => "gemini",
        ToolId::GrokBuild => "grok",
        ToolId::OpenClaw => "openclaw",
        ToolId::Hermes => "hermes",
        ToolId::Pi => "pi",
        ToolId::KimiCode => "kimi",
        ToolId::DeepSeekDsh => "dsh",
    }
}

/// Claude Desktop is a config surface rather than a standalone CLI ToolId; the other eight CLIs map reversibly.
pub fn app_type_to_tool_id(app_type: &str) -> Option<ToolId> {
    ToolId::ALL
        .into_iter()
        .find(|id| tool_id_to_app_type(*id) == Some(app_type))
}

pub fn display_name_for(id: ToolId) -> &'static str {
    match id {
        ToolId::ClaudeCode => "Claude Code",
        ToolId::Codex => "Codex CLI",
        ToolId::OpenCode => "OpenCode",
        ToolId::GeminiCli => "Gemini CLI",
        ToolId::GrokBuild => "Grok Build",
        ToolId::OpenClaw => "OpenClaw",
        ToolId::Hermes => "Hermes",
        ToolId::Pi => "Pi",
        ToolId::KimiCode => "Kimi Code",
        ToolId::DeepSeekDsh => "DeepSeek DSH",
    }
}

fn derive_status(version: &ToolVersion) -> ToolStatus {
    if version.installed_but_broken {
        return ToolStatus::Broken;
    }
    let Some(current) = version.version.as_deref() else {
        return if version.error.is_some() {
            ToolStatus::NotInstalled
        } else {
            ToolStatus::Unknown
        };
    };
    match version.latest_version.as_deref() {
        // When the version string cannot be parsed, conservatively report Installed so no false "update available" appears.
        Some(latest) => match compare_semver(latest, current) {
            Some(Ordering::Greater) => ToolStatus::UpdateAvailable,
            _ => ToolStatus::Installed,
        },
        None => ToolStatus::Installed,
    }
}

pub(crate) fn tool_from_version(id: ToolId, version: &ToolVersion) -> Tool {
    Tool {
        id,
        name: display_name_for(id).to_string(),
        description_key: format!("tool.{}.description", id.as_str()),
        discovery: ToolDiscovery::for_tool(id),
        status: derive_status(version),
        version: version.version.clone(),
        latest_version: version.latest_version.clone(),
        capabilities: capabilities_for(id),
        sessions_inside_settings: crate::compat::ccswitch::tool_paths::sessions_inside_settings(id),
        environment: if version.env_type == "unknown" {
            None
        } else {
            Some(version.env_type.clone())
        },
        configuration_shared_with: None,
    }
}

/// Call the upstream version probe and convert it into a domain Tool. Upstream encodes
/// "not installed / installed but not runnable" into ToolVersion fields rather than an Err,
/// so this cannot fail.
pub async fn detect_tool(id: ToolId) -> Tool {
    if matches!(
        id,
        ToolId::GrokBuild | ToolId::Hermes | ToolId::KimiCode | ToolId::DeepSeekDsh
    ) {
        // These tools need a local owner probe to narrow mutation capabilities
        // and, for a proven Python owner, to read the index it installs from.
        let (mut version, lifecycle_probe) = tokio::join!(
            get_single_tool_version_impl(tool_id_to_cli_name(id), None, None),
            probe(id)
        );
        if let Ok(lifecycle_probe) = &lifecycle_probe {
            latest_version::align_with_owner(id, lifecycle_probe, &mut version).await;
        }
        let mut tool = tool_from_version(id, &version);
        tool.capabilities = capabilities_after_probe(id, &lifecycle_probe);
        return tool;
    }

    let version = get_single_tool_version_impl(tool_id_to_cli_name(id), None, None).await;
    tool_from_version(id, &version)
}

/// The read-only local half: whether it is installed, which version, and whether it runs.
///
/// Not a single network request is made, so `latest_version` is always `None` and
/// `derive_status` can only yield NotInstalled / Installed / Broken / Unknown —
/// `UpdateAvailable` can only appear once the networked result of `detect_tool` arrives, and
/// the product never claims "up to date" based on this.
///
/// The probe itself blocks (one `--version` subprocess per tool), so it goes to the blocking
/// thread pool; only then is the `join_all` in `detect_all_local` really parallel rather than
/// ten tools queueing up.
pub async fn detect_tool_local(id: ToolId) -> Tool {
    let cli = tool_id_to_cli_name(id);
    let probe_local =
        tokio::task::spawn_blocking(move || probe_local_tool_version(cli, None, None));

    if matches!(
        id,
        ToolId::GrokBuild | ToolId::Hermes | ToolId::KimiCode | ToolId::DeepSeekDsh
    ) {
        // Ownership detection only looks at the filesystem and PATH and is likewise offline;
        // `align_with_owner`, which aligns latest against the owner's index, hits PyPI (30s
        // timeout) and is left to the networked phase.
        let (version, lifecycle_probe) = tokio::join!(probe_local, probe(id));
        let mut tool = tool_from_version(id, &unwrap_local_probe(cli, version));
        tool.capabilities = capabilities_after_probe(id, &lifecycle_probe);
        return tool;
    }

    tool_from_version(id, &unwrap_local_probe(cli, probe_local.await))
}

/// An unfinished ownership probe proves nothing: keep the basic capabilities of the
/// capability table, but turn off the two mutating capabilities that need ownership proof.
/// This differs from "the probe finished and found no installation" — the latter means
/// Kimi/DSH can simply install a version, while the former only means we do not know yet.
fn capabilities_after_probe(
    id: ToolId,
    lifecycle_probe: &Result<LifecycleProbe, AppError>,
) -> ToolCapabilities {
    match lifecycle_probe {
        Ok(lifecycle_probe) => capabilities_for_probe(id, lifecycle_probe),
        Err(error) => {
            log::warn!(
                "ownership probe for {} did not complete: {}",
                id.as_str(),
                error.technical_message.as_deref().unwrap_or_default()
            );
            let mut capabilities = capabilities_for(id);
            capabilities.can_uninstall = false;
            capabilities.can_manage_version = false;
            capabilities
        }
    }
}

/// When the probe thread itself did not finish (panic / cancelled), report "could not
/// determine" rather than "not installed": `version` and `error` both being None makes
/// `derive_status` yield Unknown.
fn unwrap_local_probe(
    cli: &'static str,
    joined: Result<ToolVersion, tokio::task::JoinError>,
) -> ToolVersion {
    joined.unwrap_or_else(|_| ToolVersion {
        name: cli.to_string(),
        version: None,
        latest_version: None,
        error: None,
        installed_but_broken: false,
        env_type: "unknown".to_string(),
        wsl_distro: None,
    })
}

#[cfg(test)]
mod tests {
    use super::{
        app_type_to_tool_id, capabilities_after_probe, capabilities_for, capabilities_for_probe,
        detect_tool_local, tool_from_version, tool_id_to_app_type, tool_id_to_cli_name,
    };
    use crate::commands::misc::ToolVersion;
    use crate::compat::ccswitch::install_probe::{
        HermesInstallOwner, InstallSource, InstalledEntry, LifecycleProbe,
    };
    use crate::domain::{AppError, ErrorCode, ToolId, ToolStatus};
    use crate::platform::Platform;
    use std::path::PathBuf;

    /// A probe that did not run is not a probe that found nothing: the
    /// latter lets Kimi/DSH install a version outright, the former proves no
    /// owner and keeps every ownership-gated mutation off.
    #[test]
    fn a_failed_ownership_probe_keeps_ownership_gated_mutations_off() {
        let failed = Err(AppError::new(
            ErrorCode::Internal,
            "error.command.joinFailed",
        ));
        let none_found = Ok(LifecycleProbe::default());
        for id in [ToolId::KimiCode, ToolId::DeepSeekDsh] {
            assert!(capabilities_after_probe(id, &none_found).can_manage_version);
            let unknown = capabilities_after_probe(id, &failed);
            assert!(!unknown.can_manage_version, "{id:?}");
            assert!(!unknown.can_uninstall, "{id:?}");
        }
        for id in [ToolId::GrokBuild, ToolId::Hermes] {
            let unknown = capabilities_after_probe(id, &failed);
            assert!(!unknown.can_uninstall, "{id:?}");
            assert!(!unknown.can_manage_version, "{id:?}");
            assert_eq!(unknown.can_install, capabilities_for(id).can_install);
        }
    }

    fn version_fixture() -> ToolVersion {
        ToolVersion {
            name: "claude".to_string(),
            version: Some("2.1.0".to_string()),
            latest_version: Some("2.1.0".to_string()),
            error: None,
            installed_but_broken: false,
            env_type: "macos".to_string(),
            wsl_distro: None,
        }
    }

    #[test]
    fn tool_id_and_app_type_map_both_ways() {
        for id in ToolId::ALL {
            if let Some(app_type) = tool_id_to_app_type(id) {
                assert_eq!(app_type_to_tool_id(app_type), Some(id));
            }
        }
        assert_eq!(tool_id_to_app_type(ToolId::ClaudeCode), Some("claude"));
        assert_eq!(tool_id_to_app_type(ToolId::Codex), Some("codex"));
        assert_eq!(tool_id_to_app_type(ToolId::OpenCode), Some("opencode"));
        assert_eq!(tool_id_to_app_type(ToolId::GeminiCli), Some("gemini"));
        assert_eq!(tool_id_to_app_type(ToolId::GrokBuild), Some("grokbuild"));
        assert_eq!(tool_id_to_app_type(ToolId::KimiCode), None);
        assert_eq!(tool_id_to_app_type(ToolId::DeepSeekDsh), None);
        assert_eq!(tool_id_to_cli_name(ToolId::GrokBuild), "grok");
        for (id, cli) in [(ToolId::KimiCode, "kimi"), (ToolId::DeepSeekDsh, "dsh")] {
            assert_eq!(tool_id_to_cli_name(id), cli);
        }
    }

    #[test]
    fn non_cli_surfaces_and_aliases_are_not_domain_tools() {
        for app_type in ["grok", "claude-desktop", "", "claude-code", "grok-build"] {
            assert_eq!(
                app_type_to_tool_id(app_type),
                None,
                "{app_type} must not masquerade as an AppType"
            );
        }
    }

    #[test]
    fn capabilities_follow_the_expanded_cli_table() {
        let claude = capabilities_for(ToolId::ClaudeCode);
        assert!(claude.can_install && claude.can_update && claude.can_uninstall);
        assert!(claude.can_manage_provider && claude.can_manage_mcp);
        assert!(claude.can_manage_skills && claude.can_manage_prompts);
        assert!(!claude.can_repair);
        assert_eq!(
            claude.can_launch,
            matches!(
                Platform::current(),
                Platform::MacOs | Platform::Windows | Platform::Linux
            )
        );

        let codex = capabilities_for(ToolId::Codex);
        assert!(codex.can_install && codex.can_update && codex.can_uninstall);
        assert!(codex.can_manage_provider && codex.can_manage_mcp);
        assert!(codex.can_manage_skills && codex.can_manage_prompts);
        assert_eq!(codex.can_repair, cfg!(not(target_os = "windows")));
        assert_eq!(
            codex.can_repair,
            crate::compat::ccswitch::lifecycle_specs::CODEX_NPM_REPAIR_SUPPORTED,
            "capability and repair planner platform gates drifted"
        );
        assert_eq!(codex.can_launch, claude.can_launch);

        let open_code = capabilities_for(ToolId::OpenCode);
        assert!(open_code.can_install && open_code.can_update);
        assert!(open_code.can_uninstall);
        assert!(open_code.can_manage_provider && open_code.can_manage_mcp);
        assert!(open_code.can_manage_skills && open_code.can_manage_prompts);
        assert!(!open_code.can_repair);
        assert_eq!(open_code.can_launch, claude.can_launch);

        let gemini = capabilities_for(ToolId::GeminiCli);
        assert!(gemini.can_install && gemini.can_update && gemini.can_uninstall);
        assert!(gemini.can_manage_provider && gemini.can_manage_mcp);
        assert!(gemini.can_manage_skills && gemini.can_manage_prompts);
        assert_eq!(gemini.can_launch, claude.can_launch);

        for id in [
            ToolId::GrokBuild,
            ToolId::OpenClaw,
            ToolId::Hermes,
            ToolId::Pi,
        ] {
            let capabilities = capabilities_for(id);
            assert!(
                capabilities.can_install && capabilities.can_update,
                "{id:?}"
            );
            assert_eq!(capabilities.can_launch, claude.can_launch, "{id:?}");
            assert!(!capabilities.can_repair, "{id:?}");
            assert!(capabilities.can_manage_provider, "{id:?}");
            assert!(capabilities.can_manage_prompts, "{id:?}");
        }
        assert!(capabilities_for(ToolId::GrokBuild).can_uninstall);
        assert!(capabilities_for(ToolId::OpenClaw).can_uninstall);
        assert!(capabilities_for(ToolId::Hermes).can_uninstall);
        assert!(capabilities_for(ToolId::Hermes).can_manage_version);
        assert!(capabilities_for(ToolId::Pi).can_uninstall);
        assert!(capabilities_for(ToolId::GrokBuild).can_manage_mcp);
        assert!(capabilities_for(ToolId::GrokBuild).can_manage_skills);
        assert!(!capabilities_for(ToolId::OpenClaw).can_manage_mcp);
        assert!(capabilities_for(ToolId::Hermes).can_manage_mcp);
        assert!(capabilities_for(ToolId::Hermes).can_manage_skills);
        assert!(capabilities_for(ToolId::Pi).can_manage_skills);

        for id in [ToolId::KimiCode, ToolId::DeepSeekDsh] {
            let capabilities = capabilities_for(id);
            assert!(capabilities.can_install && capabilities.can_update);
            assert_eq!(capabilities.can_launch, claude.can_launch);
            assert!(!capabilities.can_repair);
            assert!(!capabilities.can_manage_provider);
            assert!(!capabilities.can_manage_mcp);
            assert!(!capabilities.can_manage_skills);
            assert!(!capabilities.can_manage_prompts);
        }
    }

    /// Product capability gates must expose every extension adapter that the
    /// inherited core already implements. This prevents a renderer or facade
    /// from silently regressing to a smaller, hand-maintained tool list.
    #[test]
    fn extension_capabilities_cannot_fall_behind_the_inherited_app_adapters() {
        for tool in ToolId::ALL {
            let capabilities = capabilities_for(tool);
            let Some(app_id) = tool_id_to_app_type(tool) else {
                assert!(!capabilities.can_manage_mcp, "{tool:?}");
                assert!(!capabilities.can_manage_skills, "{tool:?}");
                assert!(!capabilities.can_manage_prompts, "{tool:?}");
                continue;
            };
            let app = app_id
                .parse::<crate::app_config::AppType>()
                .expect("registry AppType");

            let mut skill_apps = crate::app_config::SkillApps::default();
            skill_apps.set_enabled_for(&app, true);
            assert_eq!(
                capabilities.can_manage_skills,
                skill_apps.is_enabled_for(&app),
                "Skill gate drifted from the inherited adapter for {tool:?}"
            );

            let mut mcp_apps = crate::app_config::McpApps::default();
            mcp_apps.set_enabled_for(&app, true);
            assert_eq!(
                capabilities.can_manage_mcp,
                mcp_apps.is_enabled_for(&app),
                "MCP gate drifted from the inherited adapter for {tool:?}"
            );

            assert_eq!(
                capabilities.can_manage_prompts,
                crate::prompt_files::prompt_file_path(&app).is_ok(),
                "Prompt gate drifted from the native instruction path for {tool:?}"
            );
        }
    }

    fn grok_probe(source: InstallSource) -> LifecycleProbe {
        LifecycleProbe {
            entry: Some(InstalledEntry {
                bin_path: PathBuf::from("/Users/a/.nvm/versions/node/v22/bin/grok"),
                real_path: PathBuf::from(
                    "/Users/a/.nvm/versions/node/v22/lib/node_modules/@xai-official/grok/bin/grok",
                ),
                source,
                brew_formula: None,
                runnable: true,
                npm_package: Some("@xai-official/grok"),
                hermes_owner: None,
            }),
            path_env: None,
        }
    }

    #[test]
    fn grok_remove_capability_follows_the_detected_install_owner() {
        assert!(
            capabilities_for_probe(
                ToolId::GrokBuild,
                &grok_probe(InstallSource::NodeManagerNpm)
            )
            .can_uninstall
        );
        assert!(
            capabilities_for_probe(ToolId::GrokBuild, &grok_probe(InstallSource::Pnpm))
                .can_uninstall
        );
        assert!(
            !capabilities_for_probe(ToolId::GrokBuild, &grok_probe(InstallSource::Native))
                .can_uninstall
        );
        assert!(
            !capabilities_for_probe(ToolId::GrokBuild, &LifecycleProbe::default()).can_uninstall
        );
    }

    #[test]
    fn hermes_mutations_require_matching_python_owner_evidence() {
        let mut unmanaged = grok_probe(InstallSource::Unmanaged);
        unmanaged.entry.as_mut().unwrap().npm_package = None;
        assert!(!capabilities_for_probe(ToolId::Hermes, &unmanaged).can_uninstall);
        assert!(!capabilities_for_probe(ToolId::Hermes, &unmanaged).can_manage_version);

        let entry = unmanaged.entry.as_mut().unwrap();
        entry.source = InstallSource::UvTool;
        entry.hermes_owner = Some(HermesInstallOwner::Uv {
            program_path: PathBuf::from("/Users/a/.local/bin/uv"),
        });
        let capabilities = capabilities_for_probe(ToolId::Hermes, &unmanaged);
        assert!(capabilities.can_uninstall && capabilities.can_manage_version);

        unmanaged.entry.as_mut().unwrap().hermes_owner = Some(HermesInstallOwner::Pipx {
            program_path: PathBuf::from("/usr/bin/pipx"),
            global: false,
        });
        assert!(!capabilities_for_probe(ToolId::Hermes, &unmanaged).can_uninstall);
    }

    #[test]
    fn kimi_and_dsh_destructive_actions_follow_the_detected_owner() {
        let mut npm = grok_probe(InstallSource::NodeManagerNpm);
        npm.entry.as_mut().unwrap().npm_package = Some("@moonshot-ai/kimi-code");
        let kimi_npm = capabilities_for_probe(ToolId::KimiCode, &npm);
        assert!(kimi_npm.can_uninstall && kimi_npm.can_manage_version);

        npm.entry.as_mut().unwrap().source = InstallSource::Native;
        let kimi_native = capabilities_for_probe(ToolId::KimiCode, &npm);
        assert!(kimi_native.can_uninstall);
        assert!(!kimi_native.can_manage_version);

        npm.entry.as_mut().unwrap().source = InstallSource::NodeManagerNpm;
        npm.entry.as_mut().unwrap().npm_package = Some("@deepseek-ai/dsh");
        let dsh_npm = capabilities_for_probe(ToolId::DeepSeekDsh, &npm);
        assert!(dsh_npm.can_uninstall && dsh_npm.can_manage_version);

        npm.entry.as_mut().unwrap().source = InstallSource::Unmanaged;
        let dsh_unmanaged = capabilities_for_probe(ToolId::DeepSeekDsh, &npm);
        assert!(!dsh_unmanaged.can_uninstall);
        assert!(!dsh_unmanaged.can_manage_version);

        npm.entry.as_mut().unwrap().source = InstallSource::Brew;
        npm.entry.as_mut().unwrap().npm_package = None;
        npm.entry.as_mut().unwrap().brew_formula = Some("kimi-code".to_string());
        for source in [InstallSource::Brew, InstallSource::BrewCask] {
            npm.entry.as_mut().unwrap().source = source;
            for id in [ToolId::KimiCode, ToolId::DeepSeekDsh] {
                let brew = capabilities_for_probe(id, &npm);
                assert!(
                    brew.can_uninstall,
                    "{id:?} keeps its proven {source:?} owner"
                );
                assert!(
                    !brew.can_manage_version,
                    "{id:?} must not cross a {source:?} install into npm version management"
                );
            }
        }
    }

    #[test]
    fn status_is_derived_from_every_upstream_signal() {
        // (broken, version, latest, error, expected)
        type StatusCase = (
            bool,
            Option<&'static str>,
            Option<&'static str>,
            Option<&'static str>,
            ToolStatus,
        );
        let cases: [StatusCase; 7] = [
            (
                true,
                None,
                None,
                Some("node 16 too old"),
                ToolStatus::Broken,
            ),
            (true, Some("2.1.0"), Some("2.2.0"), None, ToolStatus::Broken),
            (
                false,
                None,
                Some("2.2.0"),
                Some("not found"),
                ToolStatus::NotInstalled,
            ),
            (false, None, None, None, ToolStatus::Unknown),
            (
                false,
                Some("2.1.0"),
                Some("2.2.0"),
                None,
                ToolStatus::UpdateAvailable,
            ),
            (
                false,
                Some("2.3.0"),
                Some("2.2.0"),
                None,
                ToolStatus::Installed,
            ),
            // When the version string cannot be parsed, conservatively report Installed so no false "update available" appears
            (
                false,
                Some("nightly"),
                Some("preview"),
                None,
                ToolStatus::Installed,
            ),
        ];
        for (broken, version, latest, error, expected) in cases {
            let probed = ToolVersion {
                name: "claude".to_string(),
                version: version.map(str::to_string),
                latest_version: latest.map(str::to_string),
                error: error.map(str::to_string),
                installed_but_broken: broken,
                env_type: "macos".to_string(),
                wsl_distro: None,
            };
            let tool = tool_from_version(ToolId::ClaudeCode, &probed);
            assert_eq!(tool.status, expected, "case {version:?}/{latest:?}");
            assert_eq!(tool.version.as_deref(), version);
            assert_eq!(tool.latest_version.as_deref(), latest);
        }

        let mut no_latest = version_fixture();
        no_latest.latest_version = None;
        assert_eq!(
            tool_from_version(ToolId::OpenCode, &no_latest).status,
            ToolStatus::Installed
        );
    }

    /// The local half is offline and therefore never knows the latest version — it must
    /// honestly leave it empty rather than imply "up to date" through "no update available".
    /// UpdateAvailable can only come from `detect_tool`.
    #[tokio::test]
    async fn the_local_half_never_claims_to_know_the_latest_version() {
        for id in ToolId::ALL {
            let tool = detect_tool_local(id).await;
            assert_eq!(tool.id, id);
            assert!(
                tool.latest_version.is_none(),
                "{} must not report a latest version without asking the network",
                id.as_str()
            );
            assert_ne!(
                tool.status,
                ToolStatus::UpdateAvailable,
                "{} cannot know an update exists yet",
                id.as_str()
            );
        }
    }

    #[test]
    #[serial_test::serial]
    fn descriptive_fields_come_from_the_domain_registry() {
        let tool = tool_from_version(ToolId::ClaudeCode, &version_fixture());
        assert_eq!(tool.id, ToolId::ClaudeCode);
        assert_eq!(tool.name, "Claude Code");
        assert_eq!(tool.description_key, "tool.claude-code.description");
        assert_eq!(tool.environment.as_deref(), Some("macos"));
        assert_eq!(tool.capabilities, capabilities_for(ToolId::ClaudeCode));

        let mut unknown_env = version_fixture();
        unknown_env.env_type = "unknown".to_string();
        assert_eq!(
            tool_from_version(ToolId::Codex, &unknown_env).environment,
            None
        );

        // This field must come from a real tool_paths computation, not a constant: a mismatch
        // means somebody wrote a per-tool-name table here.
        for id in ToolId::ALL {
            assert_eq!(
                tool_from_version(id, &version_fixture()).sessions_inside_settings,
                crate::compat::ccswitch::tool_paths::sessions_inside_settings(id),
                "{} must report the layout tool_paths actually computes",
                id.as_str()
            );
        }
    }
}
