use super::{
    install_plan, repair_plan, uninstall_plan, update_plan, GROK_INSTALL_SCRIPT,
    HERMES_INSTALL_SCRIPT, KIMI_INSTALL_SCRIPT,
};
use crate::compat::ccswitch::install_probe::{InstallSource, InstalledEntry, LifecycleProbe};
use crate::domain::ToolId;
use crate::platform::command::AllowedProgram;
use crate::platform::plan::UninstallPlan;
use std::path::PathBuf;

fn entry(source: InstallSource, bin: &str, package: Option<&'static str>) -> InstalledEntry {
    InstalledEntry {
        bin_path: PathBuf::from(bin),
        real_path: PathBuf::from(bin),
        source,
        brew_formula: None,
        runnable: true,
        npm_package: package,
        hermes_owner: None,
    }
}

fn probe(entry: Option<InstalledEntry>) -> LifecycleProbe {
    LifecycleProbe {
        entry,
        path_env: Some(("PATH".to_string(), "/usr/bin:/bin".to_string())),
    }
}

#[test]
fn expanded_official_installers_match_the_upstream_scripts() {
    assert_eq!(
        format!("bash -c '{GROK_INSTALL_SCRIPT}'"),
        crate::commands::misc::GROK_INSTALL_UNIX
    );
    assert_eq!(
        format!("bash -c '{HERMES_INSTALL_SCRIPT}'"),
        crate::commands::misc::HERMES_INSTALL_UNIX
    );
    assert_eq!(
        format!("bash -c '{KIMI_INSTALL_SCRIPT}'"),
        crate::commands::misc::KIMI_INSTALL_UNIX
    );
}

#[test]
fn every_registered_tool_has_valid_install_and_update_plans() {
    let empty = probe(None);
    for id in ToolId::ALL {
        for plan in [install_plan(id, &empty), update_plan(id, &empty, "2.0.0")] {
            let plan = plan.unwrap_or_else(|error| panic!("{id:?}: {error:?}"));
            assert!(!plan.is_empty(), "{id:?}");
            for attempt in plan.attempts {
                for step in attempt.steps {
                    step.spec
                        .validate()
                        .unwrap_or_else(|error| panic!("{id:?}: {error:?}"));
                }
            }
        }
    }
}

#[test]
fn expanded_install_sources_follow_the_upstream_priority() {
    let empty = probe(None);

    let grok = install_plan(ToolId::GrokBuild, &empty).expect("Grok install");
    assert_eq!(grok.attempts.len(), 2);
    assert_eq!(grok.attempts[0].steps[0].spec.program, AllowedProgram::Bash);
    assert_eq!(grok.attempts[1].steps[0].spec.program, AllowedProgram::Npm);

    let hermes = install_plan(ToolId::Hermes, &empty).expect("Hermes install");
    assert_eq!(hermes.attempts.len(), 1);
    assert_eq!(
        hermes.attempts[0].steps[0].spec.program,
        AllowedProgram::Bash
    );

    let kimi = install_plan(ToolId::KimiCode, &empty).expect("Kimi install");
    assert_eq!(kimi.attempts.len(), 2);
    assert_eq!(kimi.attempts[0].steps[0].spec.program, AllowedProgram::Bash);
    assert_eq!(kimi.attempts[1].steps[0].spec.program, AllowedProgram::Npm);

    let dsh = install_plan(ToolId::DeepSeekDsh, &empty).expect("DSH install");
    assert_eq!(dsh.attempts.len(), 1);
    assert_eq!(dsh.attempts[0].steps[0].spec.program, AllowedProgram::Npm);
    assert_eq!(
        dsh.attempts[0].steps[0].spec.args,
        vec!["install", "-g", "@deepseek-ai/dsh@latest"]
    );

    for id in [ToolId::GeminiCli, ToolId::OpenClaw, ToolId::Pi] {
        let plan = install_plan(id, &empty).expect("npm install");
        assert_eq!(plan.attempts.len(), 1, "{id:?}");
        assert_eq!(plan.attempts[0].steps[0].spec.program, AllowedProgram::Npm);
    }
}

#[test]
fn expanded_updates_write_back_to_the_detected_owner() {
    let openclaw = probe(Some(entry(
        InstallSource::NodeManagerNpm,
        "/Users/a/.nvm/versions/node/v22/bin/openclaw",
        Some("openclaw"),
    )));
    let plan = update_plan(ToolId::OpenClaw, &openclaw, "2.0.0").expect("OpenClaw update");
    assert_eq!(plan.attempts.len(), 2);
    assert_eq!(
        plan.attempts[0].steps[0].spec.program,
        AllowedProgram::OpenClaw
    );
    assert_eq!(
        plan.attempts[0].steps[0].spec.args,
        vec!["update".to_string(), "--yes".to_string()]
    );
    assert_eq!(plan.attempts[1].steps[0].spec.program, AllowedProgram::Npm);

    let hermes = probe(Some(entry(
        InstallSource::Unmanaged,
        "/Users/a/.local/bin/hermes",
        None,
    )));
    let plan = update_plan(ToolId::Hermes, &hermes, "2.0.0").expect("Hermes update");
    assert_eq!(plan.attempts.len(), 1);
    assert_eq!(
        plan.attempts[0].steps[0].spec.program,
        AllowedProgram::Hermes
    );

    let grok = probe(Some(entry(
        InstallSource::Native,
        "/Users/a/.grok/bin/grok",
        Some("@xai-official/grok"),
    )));
    let plan = update_plan(ToolId::GrokBuild, &grok, "2.0.0").expect("Grok update");
    assert_eq!(plan.attempts.len(), 2);
    assert_eq!(
        plan.attempts[0].steps[0].spec.program,
        AllowedProgram::GrokBuild
    );
    assert_eq!(plan.attempts[1].steps[0].spec.program, AllowedProgram::Bash);
}

#[test]
fn npm_owned_expanded_tools_can_be_removed_by_their_owner() {
    for (id, binary, package) in [
        (ToolId::GeminiCli, "gemini", "@google/gemini-cli"),
        (ToolId::GrokBuild, "grok", "@xai-official/grok"),
        (ToolId::OpenClaw, "openclaw", "openclaw"),
        (ToolId::Pi, "pi", "@earendil-works/pi-coding-agent"),
        (ToolId::KimiCode, "kimi", "@moonshot-ai/kimi-code"),
        (ToolId::DeepSeekDsh, "dsh", "@deepseek-ai/dsh"),
    ] {
        let bin = format!("/Users/a/.nvm/versions/node/v22/bin/{binary}");
        let plan = uninstall_plan(
            id,
            &probe(Some(entry(
                InstallSource::NodeManagerNpm,
                &bin,
                Some(package),
            ))),
        )
        .expect("owned uninstall");
        let UninstallPlan::Command(plan) = plan else {
            panic!("{id:?} must use npm");
        };
        assert_eq!(plan.attempts[0].steps[0].spec.program, AllowedProgram::Npm);
        assert_eq!(
            plan.attempts[0].steps[0].spec.args,
            vec![
                "uninstall".to_string(),
                "-g".to_string(),
                package.to_string()
            ]
        );
    }
}

#[test]
fn native_kimi_updates_and_uninstalls_without_crossing_to_npm() {
    let native = probe(Some(entry(
        InstallSource::Native,
        "/Users/a/.kimi-code/bin/kimi",
        Some("@moonshot-ai/kimi-code"),
    )));
    let update = update_plan(ToolId::KimiCode, &native, "2.0.0").expect("native Kimi update");
    assert_eq!(update.attempts.len(), 2);
    assert_eq!(
        update.attempts[0].steps[0].spec.program,
        AllowedProgram::KimiCode
    );
    assert_eq!(update.attempts[0].steps[0].spec.args, vec!["upgrade"]);
    assert_eq!(
        update.attempts[1].steps[0].spec.program,
        AllowedProgram::Bash
    );

    let UninstallPlan::RemovePaths(paths) =
        uninstall_plan(ToolId::KimiCode, &native).expect("native Kimi uninstall")
    else {
        panic!("native Kimi must remove only its proven launcher");
    };
    assert_eq!(paths, vec![PathBuf::from("/Users/a/.kimi-code/bin/kimi")]);
}

#[test]
fn grok_native_install_fails_closed_instead_of_removing_only_its_launcher() {
    let native = probe(Some(entry(
        InstallSource::Native,
        "/Users/a/.grok/bin/grok",
        Some("@xai-official/grok"),
    )));
    let error = uninstall_plan(ToolId::GrokBuild, &native).expect_err("unsafe partial removal");
    assert_eq!(error.code, crate::domain::ErrorCode::UninstallFailed);
}

#[test]
fn repair_is_limited_to_a_broken_codex_owned_by_sibling_npm() {
    let mut broken = entry(
        InstallSource::NodeManagerNpm,
        "/Users/a/.nvm/versions/node/v22/bin/codex",
        Some("@openai/codex"),
    );
    broken.runnable = false;
    let plan = repair_plan(ToolId::Codex, &probe(Some(broken))).expect("safe repair");
    assert_eq!(plan.attempts.len(), 1);
    assert_eq!(plan.attempts[0].steps.len(), 2);
    assert_eq!(
        plan.attempts[0].steps[0].spec.args,
        vec!["uninstall", "-g", "@openai/codex"]
    );
    assert!(plan.attempts[0].steps[0].ignore_failure);
    assert_eq!(
        plan.attempts[0].steps[1].spec.args,
        vec!["install", "-g", "@openai/codex@latest"]
    );

    let healthy = entry(
        InstallSource::NodeManagerNpm,
        "/Users/a/.nvm/versions/node/v22/bin/codex",
        Some("@openai/codex"),
    );
    assert!(repair_plan(ToolId::Codex, &probe(Some(healthy))).is_err());

    let mut unmanaged = entry(
        InstallSource::Unmanaged,
        "/usr/local/bin/codex",
        Some("@openai/codex"),
    );
    unmanaged.runnable = false;
    assert!(repair_plan(ToolId::Codex, &probe(Some(unmanaged))).is_err());
    assert!(repair_plan(ToolId::ClaudeCode, &probe(None)).is_err());
}
