use super::{
    choose_unique_claim, pipx_inventory_owns, python_name_eq, refine_with_executor,
    uv_inventory_owns,
};
use crate::compat::ccswitch::install_probe::LifecycleProbe;
use crate::compat::ccswitch::install_probe::{HermesInstallOwner, InstallSource, InstalledEntry};
use crate::domain::AppError;
use crate::platform::command::{AllowedProgram, CommandSpec};
use crate::platform::executor::{CommandExecutor, CommandOutput};
use futures::future::BoxFuture;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};

struct FixedExecutor {
    output: CommandOutput,
    seen: Mutex<Vec<CommandSpec>>,
}

impl CommandExecutor for FixedExecutor {
    fn execute(&self, spec: CommandSpec) -> BoxFuture<'static, Result<CommandOutput, AppError>> {
        self.seen.lock().expect("seen lock").push(spec);
        let output = self.output.clone();
        Box::pin(async move { Ok(output) })
    }
}

fn entry(bin: PathBuf, real: PathBuf) -> InstalledEntry {
    InstalledEntry {
        bin_path: bin,
        real_path: real,
        source: InstallSource::Unmanaged,
        brew_formula: None,
        runnable: true,
        npm_package: None,
        hermes_owner: None,
    }
}

#[test]
fn uv_requires_receipt_entrypoint_path_and_matching_environment_launcher() {
    let temp = tempfile::tempdir().unwrap();
    let exposed = temp.path().join("bin/hermes");
    let tool_dir = temp.path().join("tools/hermes-agent");
    let internal = tool_dir.join("bin/hermes");
    std::fs::create_dir_all(exposed.parent().unwrap()).unwrap();
    std::fs::create_dir_all(internal.parent().unwrap()).unwrap();
    std::fs::write(&exposed, b"launcher").unwrap();
    std::fs::write(&internal, b"launcher").unwrap();
    let inventory = format!(
        "hermes-agent v0.19.0 ({})\n- hermes ({})",
        tool_dir.display(),
        exposed.display()
    );
    assert!(uv_inventory_owns(
        &inventory,
        &entry(exposed.clone(), exposed.clone())
    ));
    std::fs::write(&exposed, b"official checkout launcher").unwrap();
    assert!(!uv_inventory_owns(
        &inventory,
        &entry(exposed.clone(), exposed)
    ));
}

#[tokio::test]
async fn read_only_refinement_anchors_inventory_to_the_discovered_manager() {
    let temp = tempfile::tempdir().unwrap();
    let bin_dir = temp.path().join("bin");
    let manager = bin_dir.join(if cfg!(target_os = "windows") {
        "uv.exe"
    } else {
        "uv"
    });
    let exposed = bin_dir.join(if cfg!(target_os = "windows") {
        "hermes.exe"
    } else {
        "hermes"
    });
    let tool_dir = temp.path().join("tools/hermes-agent");
    let internal = if cfg!(target_os = "windows") {
        tool_dir.join("Scripts/hermes.exe")
    } else {
        tool_dir.join("bin/hermes")
    };
    std::fs::create_dir_all(&bin_dir).unwrap();
    std::fs::create_dir_all(internal.parent().unwrap()).unwrap();
    std::fs::write(&manager, b"manager fixture").unwrap();
    std::fs::write(&exposed, b"launcher fixture").unwrap();
    std::fs::write(&internal, b"launcher fixture").unwrap();
    let inventory = format!(
        "hermes-agent v0.19.0 ({})\n- hermes ({})",
        tool_dir.display(),
        exposed.display()
    );
    let executor = Arc::new(FixedExecutor {
        output: CommandOutput {
            success: true,
            exit_code: Some(0),
            stdout: inventory,
            stderr: String::new(),
        },
        seen: Mutex::new(Vec::new()),
    });
    let path = std::env::join_paths([bin_dir]).unwrap();
    let mut path_only_entry = entry(exposed.clone(), exposed);
    path_only_entry.source = InstallSource::NodeManagerNpm;
    let refined = refine_with_executor(
        LifecycleProbe {
            entry: Some(path_only_entry),
            path_env: Some(("PATH".to_string(), path.to_string_lossy().into_owned())),
        },
        executor.clone(),
    )
    .await;

    let entry = refined.entry.unwrap();
    assert_eq!(entry.source, InstallSource::UvTool);
    assert_eq!(
        entry.hermes_owner,
        Some(HermesInstallOwner::Uv {
            program_path: manager.clone()
        })
    );
    let seen = executor.seen.lock().expect("seen lock");
    assert_eq!(seen.len(), 1);
    assert_eq!(seen[0].program, AllowedProgram::Uv);
    assert_eq!(seen[0].program_path, Some(manager));
    assert_eq!(seen[0].args, vec!["tool", "list", "--show-paths"]);
    assert!(seen[0]
        .env
        .iter()
        .any(|(key, value)| key == "NO_COLOR" && value == "1"));
}

#[test]
fn pipx_requires_versioned_metadata_scope_and_matching_venv_app() {
    let temp = tempfile::tempdir().unwrap();
    let bin_dir = temp.path().join("bin");
    let exposed = bin_dir.join("hermes");
    let internal = temp.path().join("venvs/hermes-agent/bin/hermes");
    std::fs::create_dir_all(&bin_dir).unwrap();
    std::fs::create_dir_all(internal.parent().unwrap()).unwrap();
    std::fs::write(&exposed, b"pipx launcher").unwrap();
    std::fs::write(&internal, b"pipx launcher").unwrap();
    let inventory = serde_json::json!({
        "pipx_spec_version": "0.1",
        "venvs": {"hermes-agent": {"metadata": {
            "exposure_enabled": true,
            "main_package": {
                "package": "hermes_agent",
                "include_apps": true,
                "suffix": "",
                "apps": ["hermes"],
                "app_paths": [{"__type__": "Path", "__Path__": internal}]
            }
        }}}
    })
    .to_string();
    assert!(pipx_inventory_owns(
        &inventory,
        &bin_dir.display().to_string(),
        &entry(exposed.clone(), exposed.clone())
    ));
    assert!(!pipx_inventory_owns(
        &inventory,
        &temp.path().join("other").display().to_string(),
        &entry(exposed.clone(), exposed)
    ));
}

#[test]
fn canonical_python_names_collapse_separator_runs() {
    assert!(python_name_eq("Hermes__Agent", "hermes-agent"));
    assert!(python_name_eq("hermes.-_agent", "hermes-agent"));
    assert!(!python_name_eq("hermes-agent-extra", "hermes-agent"));
}

#[test]
fn conflicting_manager_or_pipx_scope_claims_fail_closed() {
    let uv = HermesInstallOwner::Uv {
        program_path: PathBuf::from("/usr/bin/uv"),
    };
    let pipx = HermesInstallOwner::Pipx {
        program_path: PathBuf::from("/usr/bin/pipx"),
        global: false,
    };
    assert!(choose_unique_claim(vec![uv.clone(), pipx.clone()]).is_none());
    assert!(choose_unique_claim(vec![
        pipx,
        HermesInstallOwner::Pipx {
            program_path: PathBuf::from("/usr/bin/pipx"),
            global: true,
        }
    ])
    .is_none());
    assert!(matches!(
        choose_unique_claim(vec![uv.clone(), uv]),
        Some(HermesInstallOwner::Uv { .. })
    ));
    assert!(choose_unique_claim(vec![
        HermesInstallOwner::Uv {
            program_path: PathBuf::from("/usr/bin/uv"),
        },
        HermesInstallOwner::Uv {
            program_path: PathBuf::from("/opt/bin/uv"),
        }
    ])
    .is_none());
}
