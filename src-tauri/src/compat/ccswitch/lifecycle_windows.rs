//! Windows-only official installer specifications for lifecycle tools.
//!
//! Kept separate from `lifecycle_specs` so the cross-platform planning table stays
//! readable and under the repository's per-service size limit.

use std::time::Duration;

use crate::domain::ToolId;
use crate::platform::CommandSpec;

#[cfg(target_os = "windows")]
pub(crate) fn official_installer_spec(
    id: ToolId,
    env: Vec<(String, String)>,
    timeout: Duration,
) -> Option<CommandSpec> {
    let script = match id {
        ToolId::GrokBuild => "irm https://x.ai/cli/install.ps1 | iex",
        ToolId::Hermes => "irm https://raw.githubusercontent.com/NousResearch/hermes-agent/main/scripts/install.ps1 | iex",
        ToolId::KimiCode => "irm https://code.kimi.com/kimi-code/install.ps1 | iex",
        _ => return None,
    };
    Some(
        CommandSpec::powershell_encoded_script(script)
            .with_timeout(timeout)
            .with_env_pairs(env),
    )
}

#[cfg(not(target_os = "windows"))]
pub(crate) fn official_installer_spec(
    _id: ToolId,
    _env: Vec<(String, String)>,
    _timeout: Duration,
) -> Option<CommandSpec> {
    None
}

#[cfg(all(test, target_os = "windows"))]
mod tests {
    use std::path::PathBuf;

    use crate::compat::ccswitch::install_probe::{InstallSource, InstalledEntry, LifecycleProbe};
    use crate::domain::ToolId;

    fn broken_codex() -> LifecycleProbe {
        LifecycleProbe {
            entry: Some(InstalledEntry {
                bin_path: PathBuf::from(r"C:\Users\a\AppData\Roaming\npm\codex.cmd"),
                real_path: PathBuf::from(
                    r"C:\Users\a\AppData\Roaming\npm\node_modules\@openai\codex\bin\codex.js",
                ),
                source: InstallSource::NodeManagerNpm,
                brew_formula: None,
                runnable: false,
                npm_package: Some("@openai/codex"),
                hermes_owner: None,
            }),
            path_env: None,
        }
    }

    /// `%APPDATA%\npm`, nvm-windows and Scoop carry no POSIX manager prefix;
    /// the `npm.cmd` beside the launcher is the owner, and the update must be
    /// anchored to that exact file because `Command::new("npm")` cannot start it.
    #[test]
    fn a_launcher_beside_npm_cmd_is_npm_owned_and_anchored_to_that_npm() {
        use crate::compat::ccswitch::install_probe::classify_source;
        use crate::platform::AllowedProgram;

        let roaming = tempfile::tempdir().expect("fake Roaming\\npm");
        let codex = roaming.path().join("codex.cmd");
        std::fs::write(&codex, b"@echo off").expect("launcher");
        assert_eq!(
            classify_source(ToolId::Codex, &codex, &codex).0,
            InstallSource::Unmanaged
        );
        let npm = roaming.path().join("npm.cmd");
        std::fs::write(&npm, b"@echo off").expect("sibling npm");
        assert_eq!(
            classify_source(ToolId::Codex, &codex, &codex).0,
            InstallSource::NodeManagerNpm
        );

        let probe = LifecycleProbe {
            entry: Some(InstalledEntry {
                bin_path: codex.clone(),
                real_path: codex,
                source: InstallSource::NodeManagerNpm,
                brew_formula: None,
                runnable: true,
                npm_package: Some("@openai/codex"),
                hermes_owner: None,
            }),
            path_env: None,
        };
        let plan = crate::compat::ccswitch::lifecycle::update_plan(ToolId::Codex, &probe, "2.0.0")
            .unwrap();
        let spec = &plan.attempts[0].steps[0].spec;
        assert_eq!(spec.program, AllowedProgram::Npm);
        assert_eq!(spec.program_path, Some(npm));
    }

    /// Fresh installs, foreign-owner catalog queries and migration cleanup have
    /// no launcher to anchor to. `Command::new("npm")` never resolves `npm.cmd`
    /// (rust-lang/rust#37380), so these specs must carry the PATH `npm.cmd`.
    #[test]
    fn ownerless_npm_commands_are_anchored_to_the_path_npm_cmd() {
        use crate::compat::ccswitch::lifecycle::{install_plan, install_version_plan};
        use crate::compat::ccswitch::versioning::{catalog_target, VersionCatalogRequest};

        let bin = tempfile::tempdir().expect("fake PATH dir");
        let npm = bin.path().join("npm.cmd");
        std::fs::write(&npm, b"@echo off").expect("fake npm.cmd");
        let probe = LifecycleProbe {
            entry: None,
            path_env: Some((
                "PATH".to_string(),
                bin.path().to_string_lossy().into_owned(),
            )),
        };

        let install = install_plan(ToolId::Codex, &probe).unwrap();
        assert_eq!(
            install.attempts[0].steps[0].spec.program_path,
            Some(npm.clone())
        );
        let pinned = install_version_plan(ToolId::Codex, &probe, "1.2.3").unwrap();
        assert_eq!(
            pinned.attempts[0].steps[0].spec.program_path,
            Some(npm.clone())
        );
        let Some(VersionCatalogRequest::Npm(catalog)) =
            catalog_target(ToolId::Codex, &probe).unwrap().request
        else {
            panic!("an uninstalled npm tool still has a catalog")
        };
        assert_eq!(catalog.program_path, Some(npm));
    }

    #[test]
    fn windows_codex_damage_never_reuses_the_posix_remove_reinstall_plan() {
        let probe = broken_codex();
        assert!(crate::compat::ccswitch::lifecycle::repair_plan(ToolId::Codex, &probe).is_err());
        assert!(
            crate::compat::ccswitch::lifecycle::update_plan(ToolId::Codex, &probe, "2.0.0")
                .is_err()
        );
    }
}
