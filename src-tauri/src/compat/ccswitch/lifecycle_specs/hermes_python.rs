//! Owner-preserving Hermes lifecycle commands for proven uv-tool/pipx installs.

use crate::compat::ccswitch::install_probe::{
    HermesInstallOwner, InstallSource, InstalledEntry, LifecycleProbe,
};
use crate::platform::command::{AllowedProgram, CommandSpec};

use super::{path_env, INSTALL_TIMEOUT, UNINSTALL_TIMEOUT};

pub(crate) const PACKAGE: &str = "hermes-agent";

pub(crate) fn update(entry: &InstalledEntry, probe: &LifecycleProbe) -> Option<CommandSpec> {
    match (&entry.source, entry.hermes_owner.as_ref()) {
        (InstallSource::UvTool, Some(HermesInstallOwner::Uv { program_path })) => Some(spec(
            AllowedProgram::Uv,
            program_path.clone(),
            vec!["tool", "install", "hermes-agent@latest"],
            probe,
            INSTALL_TIMEOUT,
        )),
        (
            InstallSource::Pipx,
            Some(HermesInstallOwner::Pipx {
                program_path,
                global,
            }),
        ) => Some(spec(
            AllowedProgram::Pipx,
            program_path.clone(),
            pipx_args(*global, vec!["upgrade".to_string(), PACKAGE.to_string()]),
            probe,
            INSTALL_TIMEOUT,
        )),
        _ => None,
    }
}

pub(crate) fn uninstall(entry: &InstalledEntry, probe: &LifecycleProbe) -> Option<CommandSpec> {
    match (&entry.source, entry.hermes_owner.as_ref()) {
        (InstallSource::UvTool, Some(HermesInstallOwner::Uv { program_path })) => Some(spec(
            AllowedProgram::Uv,
            program_path.clone(),
            vec!["tool", "uninstall", PACKAGE],
            probe,
            UNINSTALL_TIMEOUT,
        )),
        (
            InstallSource::Pipx,
            Some(HermesInstallOwner::Pipx {
                program_path,
                global,
            }),
        ) => Some(spec(
            AllowedProgram::Pipx,
            program_path.clone(),
            pipx_args(*global, vec!["uninstall".to_string(), PACKAGE.to_string()]),
            probe,
            UNINSTALL_TIMEOUT,
        )),
        _ => None,
    }
}

pub(crate) fn install_version(
    entry: &InstalledEntry,
    probe: &LifecycleProbe,
    version: &str,
) -> Option<CommandSpec> {
    let package = format!("{PACKAGE}=={version}");
    match (&entry.source, entry.hermes_owner.as_ref()) {
        (InstallSource::UvTool, Some(HermesInstallOwner::Uv { program_path })) => Some(spec(
            AllowedProgram::Uv,
            program_path.clone(),
            vec!["tool".to_string(), "install".to_string(), package],
            probe,
            INSTALL_TIMEOUT,
        )),
        (
            InstallSource::Pipx,
            Some(HermesInstallOwner::Pipx {
                program_path,
                global,
            }),
        ) => Some(spec(
            AllowedProgram::Pipx,
            program_path.clone(),
            pipx_args(
                *global,
                vec!["install".to_string(), "--force".to_string(), package],
            ),
            probe,
            INSTALL_TIMEOUT,
        )),
        _ => None,
    }
}

fn spec<S: Into<String>>(
    program: AllowedProgram,
    program_path: std::path::PathBuf,
    args: Vec<S>,
    probe: &LifecycleProbe,
    timeout: std::time::Duration,
) -> CommandSpec {
    CommandSpec::new(program, args.into_iter().map(Into::into).collect())
        .with_program_path(program_path)
        .with_timeout(timeout)
        .with_env_pairs(path_env(probe))
}

fn pipx_args(global: bool, mut args: Vec<String>) -> Vec<String> {
    if global {
        args.insert(0, "--global".to_string());
    }
    args
}

#[cfg(test)]
mod tests {
    use super::{install_version, uninstall, update};
    use crate::compat::ccswitch::install_probe::{
        HermesInstallOwner, InstallSource, InstalledEntry, LifecycleProbe,
    };
    use crate::platform::command::AllowedProgram;
    use std::path::PathBuf;

    fn probe(source: InstallSource, owner: HermesInstallOwner) -> LifecycleProbe {
        LifecycleProbe {
            entry: Some(InstalledEntry {
                bin_path: PathBuf::from("/Users/a/.local/bin/hermes"),
                real_path: PathBuf::from("/Users/a/.local/bin/hermes"),
                source,
                brew_formula: None,
                runnable: true,
                npm_package: None,
                hermes_owner: Some(owner),
            }),
            path_env: Some(("PATH".to_string(), "/usr/bin:/bin".to_string())),
        }
    }

    #[test]
    fn uv_actions_stay_anchored_and_replace_an_exact_pin_on_update() {
        let probe = probe(
            InstallSource::UvTool,
            HermesInstallOwner::Uv {
                program_path: PathBuf::from("/Users/a/.local/bin/uv"),
            },
        );
        let entry = probe.entry.as_ref().unwrap();
        let update = update(entry, &probe).unwrap();
        assert_eq!(update.program, AllowedProgram::Uv);
        assert_eq!(update.args, vec!["tool", "install", "hermes-agent@latest"]);
        assert_eq!(
            update.program_path,
            Some(PathBuf::from("/Users/a/.local/bin/uv"))
        );
        assert_eq!(
            install_version(entry, &probe, "0.19.0").unwrap().args,
            vec!["tool", "install", "hermes-agent==0.19.0"]
        );
        assert_eq!(
            uninstall(entry, &probe).unwrap().args,
            vec!["tool", "uninstall", "hermes-agent"]
        );
    }

    #[test]
    fn pipx_global_scope_is_preserved_for_every_action() {
        let probe = probe(
            InstallSource::Pipx,
            HermesInstallOwner::Pipx {
                program_path: PathBuf::from("/opt/homebrew/bin/pipx"),
                global: true,
            },
        );
        let entry = probe.entry.as_ref().unwrap();
        assert_eq!(
            update(entry, &probe).unwrap().args,
            vec!["--global", "upgrade", "hermes-agent"]
        );
        assert_eq!(
            install_version(entry, &probe, "0.18.0").unwrap().args,
            vec!["--global", "install", "--force", "hermes-agent==0.18.0"]
        );
        assert_eq!(
            uninstall(entry, &probe).unwrap().args,
            vec!["--global", "uninstall", "hermes-agent"]
        );
    }

    #[test]
    fn a_source_without_its_matching_owner_evidence_cannot_plan() {
        let mut probe = probe(
            InstallSource::UvTool,
            HermesInstallOwner::Pipx {
                program_path: PathBuf::from("/usr/bin/pipx"),
                global: false,
            },
        );
        let entry = probe.entry.take().unwrap();
        assert!(update(&entry, &probe).is_none());
        assert!(uninstall(&entry, &probe).is_none());
        assert!(install_version(&entry, &probe, "0.19.0").is_none());
    }
}
