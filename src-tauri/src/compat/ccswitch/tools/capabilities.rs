use crate::compat::ccswitch::install_probe::{
    HermesInstallOwner, InstallSource, InstalledEntry, LifecycleProbe,
};
use crate::domain::{ModelChoiceSpec, ToolCapabilities, ToolId};
use crate::platform::Platform;

/// The capability table of spec §12. Repair is only enabled when there is a verified recovery plan that does not guess the install ownership.
/// What Home may choose comes from the model-choice table (ADR-0054), so the two cannot drift.
pub fn capabilities_for(id: ToolId) -> ToolCapabilities {
    let choice = ModelChoiceSpec::for_tool(id);
    ToolCapabilities {
        can_choose_model: choice.is_some(),
        can_choose_effort: choice.is_some_and(|spec| spec.has_effort()),
        ..lifecycle_capabilities(id)
    }
}

fn lifecycle_capabilities(id: ToolId) -> ToolCapabilities {
    let can_launch = platform_supports_launch(Platform::current());
    match id {
        ToolId::ClaudeCode => ToolCapabilities {
            can_install: true,
            can_update: true,
            can_uninstall: true,
            can_repair: false,
            can_launch,
            can_manage_provider: true,
            can_manage_mcp: true,
            can_manage_skills: true,
            can_manage_prompts: true,
            can_manage_version: true,
            ..ToolCapabilities::default()
        },
        ToolId::Codex => ToolCapabilities {
            can_install: true,
            can_update: true,
            can_uninstall: true,
            can_repair: cfg!(not(target_os = "windows")),
            can_launch,
            can_manage_provider: true,
            can_manage_mcp: true,
            can_manage_skills: true,
            can_manage_prompts: true,
            can_manage_version: true,
            ..ToolCapabilities::default()
        },
        ToolId::OpenCode => ToolCapabilities {
            can_install: true,
            can_update: true,
            can_uninstall: true,
            can_repair: false,
            can_launch,
            can_manage_provider: true,
            can_manage_mcp: true,
            can_manage_skills: true,
            can_manage_prompts: true,
            can_manage_version: true,
            ..ToolCapabilities::default()
        },
        ToolId::GeminiCli => ToolCapabilities {
            can_install: true,
            can_update: true,
            can_uninstall: true,
            can_repair: false,
            can_launch,
            can_manage_provider: true,
            can_manage_mcp: true,
            can_manage_skills: true,
            can_manage_prompts: true,
            can_manage_version: true,
            ..ToolCapabilities::default()
        },
        ToolId::GrokBuild => ToolCapabilities {
            can_install: true,
            can_update: true,
            // Static application gates stay open; detected UI capabilities and
            // every mutation are narrowed again by the owner proof below.
            can_uninstall: true,
            can_repair: false,
            can_launch,
            can_manage_provider: true,
            can_manage_mcp: true,
            can_manage_skills: true,
            can_manage_prompts: true,
            can_manage_version: true,
            ..ToolCapabilities::default()
        },
        ToolId::OpenClaw => ToolCapabilities {
            can_install: true,
            can_update: true,
            can_uninstall: true,
            can_repair: false,
            can_launch,
            can_manage_provider: true,
            // Upstream explicitly states that OpenClaw currently has no native MCP/skills registry.
            can_manage_mcp: false,
            can_manage_skills: false,
            // OpenClaw has no MCP/Skills registry, but it does read the
            // workspace-level AGENTS.md managed by the Prompt service.
            can_manage_prompts: true,
            can_manage_version: true,
            ..ToolCapabilities::default()
        },
        ToolId::Hermes => ToolCapabilities {
            can_install: true,
            can_update: true,
            can_uninstall: true,
            can_repair: false,
            can_launch,
            can_manage_provider: true,
            can_manage_mcp: true,
            can_manage_skills: true,
            can_manage_prompts: true,
            can_manage_version: true,
            ..ToolCapabilities::default()
        },
        ToolId::Pi => ToolCapabilities {
            can_install: true,
            can_update: true,
            can_uninstall: true,
            can_repair: false,
            can_launch,
            can_manage_provider: true,
            // Pi core has no native MCP registry, but upstream has verified the skills directory.
            can_manage_mcp: false,
            can_manage_skills: true,
            // Pi derives the selected Prompt from its native AGENTS.md rather
            // than a persisted enabled flag. The Prompt facade has a dedicated
            // revision-checked write path for that model.
            can_manage_prompts: true,
            can_manage_version: true,
            ..ToolCapabilities::default()
        },
        ToolId::KimiCode => ToolCapabilities {
            can_install: true,
            can_update: true,
            // The detected projection narrows this to the official native
            // layout or a package-manager-owned launcher.
            can_uninstall: true,
            can_repair: false,
            can_launch,
            can_manage_provider: false,
            can_manage_mcp: false,
            can_manage_skills: false,
            can_manage_prompts: false,
            can_manage_version: true,
            ..ToolCapabilities::default()
        },
        ToolId::DeepSeekDsh => ToolCapabilities {
            can_install: true,
            can_update: true,
            // DSH has no documented native uninstaller. Only a proven package
            // manager owner may expose destructive or version mutations.
            can_uninstall: true,
            can_repair: false,
            can_launch,
            can_manage_provider: false,
            can_manage_mcp: false,
            can_manage_skills: false,
            can_manage_prompts: false,
            can_manage_version: true,
            ..ToolCapabilities::default()
        },
    }
}

fn platform_supports_launch(platform: Platform) -> bool {
    matches!(
        platform,
        Platform::MacOs | Platform::Windows | Platform::Linux
    )
}

/// Grok and Hermes expose destructive/version mutations only after proving
/// that the selected launcher belongs to the matching package manager.
pub(crate) fn capabilities_for_probe(
    id: ToolId,
    lifecycle_probe: &LifecycleProbe,
) -> ToolCapabilities {
    let mut capabilities = capabilities_for(id);
    if id == ToolId::GrokBuild {
        capabilities.can_uninstall = lifecycle_probe
            .entry
            .as_ref()
            .is_some_and(grok_package_owner);
    } else if id == ToolId::Hermes {
        let managed = lifecycle_probe
            .entry
            .as_ref()
            .is_some_and(hermes_python_owner);
        capabilities.can_uninstall = managed;
        capabilities.can_manage_version = managed;
    } else if id == ToolId::KimiCode {
        let source = lifecycle_probe.entry.as_ref().map(|entry| entry.source);
        capabilities.can_uninstall = lifecycle_probe
            .entry
            .as_ref()
            .is_some_and(|entry| entry.source == InstallSource::Native || package_owner(entry));
        capabilities.can_manage_version = source.is_none()
            || lifecycle_probe
                .entry
                .as_ref()
                .is_some_and(node_package_owner);
    } else if id == ToolId::DeepSeekDsh {
        let owner = lifecycle_probe.entry.as_ref().is_some_and(package_owner);
        capabilities.can_uninstall = owner;
        capabilities.can_manage_version = lifecycle_probe.entry.is_none()
            || lifecycle_probe
                .entry
                .as_ref()
                .is_some_and(node_package_owner);
    }
    capabilities
}

fn package_owner(entry: &InstalledEntry) -> bool {
    node_package_owner(entry)
        || matches!(entry.source, InstallSource::Brew | InstallSource::BrewCask)
            && entry.brew_formula.is_some()
}

fn node_package_owner(entry: &InstalledEntry) -> bool {
    match entry.source {
        InstallSource::NodeManagerNpm
        | InstallSource::Volta
        | InstallSource::Bun
        | InstallSource::Pnpm => entry.npm_package.is_some(),
        InstallSource::Brew | InstallSource::BrewCask => false,
        InstallSource::UvTool
        | InstallSource::Pipx
        | InstallSource::Native
        | InstallSource::Unmanaged => false,
    }
}

fn grok_package_owner(entry: &InstalledEntry) -> bool {
    match entry.source {
        InstallSource::NodeManagerNpm
        | InstallSource::Volta
        | InstallSource::Bun
        | InstallSource::Pnpm => entry.npm_package.is_some(),
        InstallSource::Brew | InstallSource::BrewCask => entry.brew_formula.is_some(),
        InstallSource::UvTool
        | InstallSource::Pipx
        | InstallSource::Native
        | InstallSource::Unmanaged => false,
    }
}

fn hermes_python_owner(entry: &InstalledEntry) -> bool {
    matches!(
        (&entry.source, entry.hermes_owner.as_ref()),
        (InstallSource::UvTool, Some(HermesInstallOwner::Uv { .. }))
            | (InstallSource::Pipx, Some(HermesInstallOwner::Pipx { .. }))
    )
}

#[cfg(test)]
mod tests {
    use super::{capabilities_for, platform_supports_launch};
    use crate::domain::ToolId;
    use crate::platform::Platform;

    #[test]
    fn home_chooses_a_model_only_where_it_also_manages_endpoints() {
        for tool in ToolId::ALL {
            let capabilities = capabilities_for(tool);
            if capabilities.can_choose_model {
                assert!(capabilities.can_manage_provider, "{tool:?}");
            }
            if capabilities.can_choose_effort {
                assert!(capabilities.can_choose_model, "{tool:?}");
            }
        }
        assert!(capabilities_for(ToolId::Codex).can_choose_effort);
        assert!(!capabilities_for(ToolId::GeminiCli).can_choose_effort);
        assert!(!capabilities_for(ToolId::OpenCode).can_choose_model);
    }

    #[test]
    fn every_shipped_desktop_platform_can_launch_tools() {
        for platform in [Platform::MacOs, Platform::Windows, Platform::Linux] {
            assert!(platform_supports_launch(platform));
        }
        assert!(!platform_supports_launch(Platform::Unknown));
    }
}
