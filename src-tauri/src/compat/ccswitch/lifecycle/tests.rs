use super::{
    install_plan, uninstall_plan, update_plan, CLAUDE_INSTALL_SCRIPT, CODEX_INSTALL_SCRIPT,
    OPENCODE_INSTALL_SCRIPT,
};
use crate::compat::ccswitch::install_probe::{InstallSource, InstalledEntry, LifecycleProbe};
use crate::domain::{ErrorCode, ToolId};
use crate::platform::command::AllowedProgram;
use crate::platform::plan::{LifecyclePlan, UninstallPlan};
use std::path::PathBuf;

fn entry(source: InstallSource, bin: &str, package: &'static str) -> InstalledEntry {
    InstalledEntry {
        bin_path: PathBuf::from(bin),
        real_path: PathBuf::from(bin),
        source,
        brew_formula: None,
        runnable: true,
        npm_package: Some(package),
        hermes_owner: None,
    }
}

/// The standalone Codex installer: launcher in `~/.local/bin`, real binary in
/// `~/.codex/packages/standalone/releases/<ver>/codex`.
fn standalone_codex() -> InstalledEntry {
    InstalledEntry {
        real_path: PathBuf::from("/Users/a/.codex/packages/standalone/releases/0.100.0/codex"),
        ..entry(
            InstallSource::Native,
            "/Users/a/.local/bin/codex",
            "@openai/codex",
        )
    }
}

fn probe_with(entry: InstalledEntry) -> LifecycleProbe {
    LifecycleProbe {
        entry: Some(entry),
        path_env: Some(("PATH".to_string(), "/usr/bin:/bin".to_string())),
    }
}

fn empty_probe() -> LifecycleProbe {
    LifecycleProbe {
        entry: None,
        path_env: Some(("PATH".to_string(), "/usr/bin:/bin".to_string())),
    }
}

fn programs(plan: &LifecyclePlan) -> Vec<(AllowedProgram, Vec<String>)> {
    plan.attempts
        .iter()
        .flat_map(|attempt| attempt.steps.iter())
        .map(|step| (step.spec.program, step.spec.args.clone()))
        .collect()
}

#[test]
fn every_generated_spec_passes_validation() {
    let probes = [
        empty_probe(),
        probe_with(entry(
            InstallSource::NodeManagerNpm,
            "/Users/a/.nvm/versions/node/v20/bin/claude",
            "@anthropic-ai/claude-code",
        )),
        probe_with(entry(
            InstallSource::Native,
            "/Users/a/.local/bin/claude",
            "@anthropic-ai/claude-code",
        )),
    ];
    for probe in &probes {
        for plan in [
            install_plan(ToolId::ClaudeCode, probe),
            update_plan(ToolId::ClaudeCode, probe, "2.0.0"),
        ]
        .into_iter()
        .flatten()
        {
            assert!(!plan.is_empty());
            for attempt in &plan.attempts {
                for step in &attempt.steps {
                    step.spec
                        .validate()
                        .unwrap_or_else(|e| panic!("invalid spec: {e:?}"));
                }
            }
        }
    }
}

/// ADR-0033: the product scripts are the upstream scripts plus curl time limits,
/// so a blocked host fails within seconds and the next attempt starts instead of
/// holding the task until the 15-minute step timeout.
#[test]
fn the_official_installer_scripts_are_upstream_plus_curl_time_limits() {
    const LIMITS: &str = "curl -fsSL --connect-timeout 10 --max-time 120 ";
    for (script, upstream) in [
        (
            CLAUDE_INSTALL_SCRIPT,
            crate::commands::misc::CLAUDE_INSTALL_UNIX,
        ),
        (
            OPENCODE_INSTALL_SCRIPT,
            crate::commands::misc::OPENCODE_INSTALL_UNIX,
        ),
    ] {
        assert!(script.contains(LIMITS), "{script}");
        assert_eq!(
            format!("bash -c '{}'", script.replace(LIMITS, "curl -fsSL ")),
            upstream
        );
    }
    assert!(CODEX_INSTALL_SCRIPT.contains(LIMITS));
    assert!(CODEX_INSTALL_SCRIPT.contains(
        "https://raw.githubusercontent.com/openai/codex/main/scripts/install/install.sh"
    ));
    assert!(CODEX_INSTALL_SCRIPT
        .contains("CODEX_NON_INTERACTIVE=1 CODEX_INSTALLER_USE_RELEASES_OPENAI_COM=false sh $tmp"));
    for script in [
        CLAUDE_INSTALL_SCRIPT,
        OPENCODE_INSTALL_SCRIPT,
        CODEX_INSTALL_SCRIPT,
    ] {
        assert!(!script.contains('\0'));
        assert!(!script.contains("{}"), "no interpolation slot may exist");
        assert!(!script.contains("| bash") && !script.contains("| sh"));
    }
}

#[test]
fn the_npm_argv_matches_the_upstream_command_string() {
    for (id, app_type) in [
        (ToolId::ClaudeCode, "claude"),
        (ToolId::Codex, "codex"),
        (ToolId::OpenCode, "opencode"),
    ] {
        let package =
            crate::commands::misc::npm_package_for(app_type).expect("upstream package name");
        let expected = format!("npm i -g {package}@latest");
        assert_eq!(
            crate::commands::misc::npm_install_command_for(app_type),
            Some(expected.as_str()),
            "{app_type}"
        );
        let plan = install_plan(id, &empty_probe()).expect("install plan");
        let npm_step = programs(&plan)
            .into_iter()
            .find(|(program, _)| *program == AllowedProgram::Npm)
            .expect("an npm attempt exists");
        assert_eq!(
            npm_step.1,
            vec![
                "install".to_string(),
                "-g".to_string(),
                format!("{package}@latest")
            ]
        );
    }
}

#[test]
fn claude_opencode_and_codex_install_try_the_official_script_before_npm() {
    for (id, script) in [
        (ToolId::ClaudeCode, CLAUDE_INSTALL_SCRIPT),
        (ToolId::OpenCode, OPENCODE_INSTALL_SCRIPT),
        (ToolId::Codex, CODEX_INSTALL_SCRIPT),
    ] {
        let plan = install_plan(id, &empty_probe()).expect("install plan");
        assert_eq!(plan.attempts.len(), 2);
        assert_eq!(plan.attempts[0].steps[0].spec.program, AllowedProgram::Bash);
        assert_eq!(
            plan.attempts[0].steps[0].spec.args,
            vec!["-c".to_string(), script.to_string()]
        );
        assert_eq!(plan.attempts[1].steps[0].spec.program, AllowedProgram::Npm);
    }
}

#[test]
fn an_anchored_npm_update_carries_the_absolute_npm_and_a_prefixed_path() {
    let probe = probe_with(entry(
        InstallSource::NodeManagerNpm,
        "/Users/a/.nvm/versions/node/v20/bin/codex",
        "@openai/codex",
    ));
    let plan = update_plan(ToolId::Codex, &probe, "2.0.0").expect("update plan");
    let spec = &plan.attempts[0].steps[0].spec;
    assert_eq!(spec.program, AllowedProgram::Npm);
    assert_eq!(
        spec.program_path,
        Some(PathBuf::from("/Users/a/.nvm/versions/node/v20/bin/npm"))
    );
    let path = spec
        .env
        .iter()
        .find(|(key, _)| key == "PATH")
        .map(|(_, value)| value.clone())
        .expect("PATH is injected");
    assert!(
        path.starts_with("/Users/a/.nvm/versions/node/v20/bin"),
        "the npm directory must come first so env node resolves there: {path}"
    );
}

#[test]
fn grok_package_actions_use_the_highest_stable_range() {
    let install = install_plan(ToolId::GrokBuild, &empty_probe()).expect("install plan");
    let install_spec = programs(&install)
        .into_iter()
        .find(|(program, _)| *program == AllowedProgram::Npm)
        .expect("npm fallback");
    assert_eq!(
        install_spec.1,
        vec!["install", "-g", "@xai-official/grok@*"]
    );

    let probe = probe_with(entry(
        InstallSource::NodeManagerNpm,
        "/Users/a/.nvm/versions/node/v20/bin/grok",
        "@xai-official/grok",
    ));
    // The update target is the checked recommendation (highest published
    // stable for Grok), so the plan pins it instead of repeating the range.
    let update = update_plan(ToolId::GrokBuild, &probe, "1.0.12").expect("update plan");
    let update_spec = programs(&update)
        .into_iter()
        .find(|(program, _)| *program == AllowedProgram::Npm)
        .expect("npm update fallback");
    assert_eq!(
        update_spec.1,
        vec!["install", "-g", "@xai-official/grok@1.0.12"]
    );
}

/// The checked preview may recommend a prerelease tag (upstream picks `next`
/// when the local build is ahead of `latest`). Installing `@latest` would then
/// downgrade the user and could never verify against the authorised target.
#[test]
fn an_update_installs_the_checked_target_instead_of_the_latest_tag() {
    let package = "@anthropic-ai/claude-code";
    let cases = [
        (
            InstallSource::NodeManagerNpm,
            "/Users/a/.nvm/versions/node/v20/bin/claude",
            AllowedProgram::Npm,
            vec!["install", "-g", "@anthropic-ai/claude-code@2.1.101"],
        ),
        (
            InstallSource::Volta,
            "/Users/a/.volta/bin/claude",
            AllowedProgram::Volta,
            vec!["install", "@anthropic-ai/claude-code@2.1.101"],
        ),
        (
            InstallSource::Bun,
            "/Users/a/.bun/bin/claude",
            AllowedProgram::Bun,
            vec!["add", "-g", "@anthropic-ai/claude-code@2.1.101"],
        ),
        (
            InstallSource::Pnpm,
            "/Users/a/Library/pnpm/claude",
            AllowedProgram::Pnpm,
            vec!["add", "-g", "@anthropic-ai/claude-code@2.1.101"],
        ),
    ];
    for (source, bin, program, args) in cases {
        let plan = update_plan(
            ToolId::ClaudeCode,
            &probe_with(entry(source, bin, package)),
            "2.1.101",
        )
        .expect("update plan");
        let step = programs(&plan)
            .into_iter()
            .find(|(candidate, _)| *candidate == program)
            .unwrap_or_else(|| panic!("{source:?} must keep its package manager attempt"));
        assert_eq!(step.1, args, "{source:?}");
    }

    // The fallback for an undetected default install pins the same target.
    let fallback = update_plan(ToolId::Codex, &empty_probe(), "0.5.0").expect("static plan");
    assert_eq!(
        programs(&fallback)[0].1,
        vec!["install", "-g", "@openai/codex@0.5.0"]
    );
    let error =
        update_plan(ToolId::Codex, &empty_probe(), "latest").expect_err("a tag is not a version");
    assert_eq!(error.message_key, "error.tool.versionInvalid");
}

/// ADR-0033: where the registry can supply the official layout, re-running the
/// official installer is redundant (it reaches the same blocked host), so the
/// plan is the single self-update and the adapter supplies the layout after it.
#[test]
fn a_native_install_updates_through_the_tool_itself() {
    let probe = probe_with(entry(
        InstallSource::Native,
        "/Users/a/.local/bin/claude",
        "@anthropic-ai/claude-code",
    ));
    let plan = update_plan(ToolId::ClaudeCode, &probe, "2.0.0").expect("update plan");
    let spec = &plan.attempts[0].steps[0].spec;
    assert_eq!(spec.program, AllowedProgram::ClaudeCode);
    assert_eq!(spec.args, vec!["update".to_string()]);
    assert_eq!(
        spec.program_path,
        Some(PathBuf::from("/Users/a/.local/bin/claude"))
    );
    if crate::compat::ccswitch::native_supply::supplies(ToolId::ClaudeCode) {
        assert_eq!(
            plan.attempts.len(),
            1,
            "supply replaces the installer retry"
        );
    } else {
        assert_eq!(plan.attempts.len(), 2);
        assert_eq!(plan.attempts[1].steps[0].spec.program, AllowedProgram::Bash);
        assert_eq!(
            plan.attempts[1].steps[0].spec.args,
            vec!["-c".to_string(), CLAUDE_INSTALL_SCRIPT.to_string()]
        );
    }
}

#[test]
fn a_brew_install_upgrades_the_formula_through_the_sibling_brew() {
    let mut brewed = entry(
        InstallSource::Brew,
        "/opt/homebrew/bin/gemini",
        "@google/gemini-cli",
    );
    brewed.brew_formula = Some("gemini-cli".to_string());
    let plan = update_plan(ToolId::GeminiCli, &probe_with(brewed), "2.0.0").expect("update plan");
    let spec = &plan.attempts[0].steps[0].spec;
    assert_eq!(spec.program, AllowedProgram::Brew);
    assert_eq!(
        spec.args,
        vec!["upgrade".to_string(), "gemini-cli".to_string()]
    );
    assert_eq!(
        spec.program_path,
        Some(PathBuf::from("/opt/homebrew/bin/brew"))
    );
}

#[test]
fn a_cask_install_upgrades_and_uninstalls_as_a_cask_through_the_sibling_brew() {
    let mut cask = entry(
        InstallSource::BrewCask,
        "/opt/homebrew/bin/claude",
        "@anthropic-ai/claude-code",
    );
    cask.real_path = PathBuf::from("/opt/homebrew/Caskroom/claude-code/2.1.236/claude");
    cask.brew_formula = Some("claude-code".to_string());

    let plan =
        update_plan(ToolId::ClaudeCode, &probe_with(cask.clone()), "2.1.237").expect("update plan");
    assert_eq!(
        plan.attempts.len(),
        1,
        "no self-update and no npm crossover"
    );
    let spec = &plan.attempts[0].steps[0].spec;
    assert_eq!(spec.program, AllowedProgram::Brew);
    assert_eq!(spec.args, vec!["upgrade", "--cask", "claude-code"]);
    assert_eq!(
        spec.program_path,
        Some(PathBuf::from("/opt/homebrew/bin/brew"))
    );

    let UninstallPlan::Command(commands) =
        uninstall_plan(ToolId::ClaudeCode, &probe_with(cask)).expect("uninstall plan")
    else {
        panic!("a cask is removed by brew, not by deleting files");
    };
    let spec = &commands.attempts[0].steps[0].spec;
    assert_eq!(spec.args, vec!["uninstall", "--cask", "claude-code"]);
    spec.validate().expect("valid cask uninstall spec");
}

#[test]
fn opencode_prefers_its_own_upgrade_then_falls_back_to_the_package_manager() {
    let probe = probe_with(entry(
        InstallSource::NodeManagerNpm,
        "/Users/a/.nvm/versions/node/v20/bin/opencode",
        "opencode-ai",
    ));
    let plan = update_plan(ToolId::OpenCode, &probe, "2.0.0").expect("update plan");
    assert_eq!(plan.attempts.len(), 2);
    assert_eq!(
        plan.attempts[0].steps[0].spec.program,
        AllowedProgram::OpenCode
    );
    assert_eq!(
        plan.attempts[0].steps[0].spec.args,
        vec!["upgrade".to_string()]
    );
    assert_eq!(plan.attempts[1].steps[0].spec.program, AllowedProgram::Npm);
}

/// ADR-0033: a standalone Codex install is owned by its own updater, which in
/// practice runs the official install script; the product then retries the
/// repository copy of that script forced onto GitHub Releases.
#[test]
fn a_standalone_codex_install_updates_through_its_updater_then_the_official_script() {
    let plan = update_plan(ToolId::Codex, &probe_with(standalone_codex()), "2.0.0")
        .expect("standalone Codex update plan");
    assert_eq!(plan.attempts.len(), 2);
    let self_update = &plan.attempts[0].steps[0].spec;
    assert_eq!(self_update.program, AllowedProgram::Codex);
    assert_eq!(self_update.args, vec!["update".to_string()]);
    assert_eq!(
        self_update.program_path,
        Some(PathBuf::from("/Users/a/.local/bin/codex"))
    );
    assert!(self_update
        .env
        .iter()
        .any(|(key, value)| key == "CODEX_NON_INTERACTIVE" && value == "1"));
    let script = &plan.attempts[1].steps[0].spec;
    assert_eq!(script.program, AllowedProgram::Bash);
    assert_eq!(
        script.args,
        vec!["-c".to_string(), CODEX_INSTALL_SCRIPT.to_string()]
    );
    assert!(script.args[1].contains("CODEX_INSTALLER_USE_RELEASES_OPENAI_COM=false"));
    script.validate().expect("sealed official script");
}

#[test]
fn npm_owned_codex_never_uses_its_own_self_update() {
    let probe = probe_with(entry(
        InstallSource::NodeManagerNpm,
        "/Users/a/.nvm/versions/node/v20/bin/codex",
        "@openai/codex",
    ));
    let plan = update_plan(ToolId::Codex, &probe, "2.0.0").expect("update plan");
    assert!(
        programs(&plan)
            .iter()
            .all(|(program, _)| *program != AllowedProgram::Codex),
        "codex update exit-0s on a broken platform binary (upstream misc.rs:2723)"
    );
}

#[test]
fn a_broken_codex_npm_install_is_repaired_by_uninstall_then_install() {
    let mut broken = entry(
        InstallSource::NodeManagerNpm,
        "/Users/a/.nvm/versions/node/v20/bin/codex",
        "@openai/codex",
    );
    broken.runnable = false;
    let plan = update_plan(ToolId::Codex, &probe_with(broken), "2.0.0").expect("update plan");
    assert_eq!(plan.attempts.len(), 1);
    let steps = &plan.attempts[0].steps;
    assert_eq!(steps.len(), 2);
    assert!(steps[0].ignore_failure, "a failed uninstall must not abort");
    assert_eq!(
        steps[0].spec.args,
        vec![
            "uninstall".to_string(),
            "-g".to_string(),
            "@openai/codex".to_string()
        ]
    );
    assert!(!steps[1].ignore_failure);
    assert_eq!(
        steps[1].spec.args,
        vec![
            "install".to_string(),
            "-g".to_string(),
            "@openai/codex@latest".to_string()
        ]
    );
}

#[test]
fn the_codex_repair_gate_agrees_with_upstream() {
    use crate::compat::ccswitch::install_probe::classify_source;
    use std::path::Path;

    let cases = [
        (
            "/Users/a/.nvm/versions/node/v20/bin/codex",
            "/Users/a/.nvm/versions/node/v20/lib/node_modules/@openai/codex/bin/codex.js",
            true,
        ),
        (
            "/opt/homebrew/bin/codex",
            "/opt/homebrew/lib/node_modules/@openai/codex/bin/codex.js",
            true,
        ),
        (
            "/opt/homebrew/bin/codex",
            "/opt/homebrew/Cellar/codex/1.0/bin/codex",
            false,
        ),
        (
            "/Users/a/.volta/bin/codex",
            "/Users/a/.volta/tools/image/packages/@openai/codex/bin/codex",
            false,
        ),
        (
            "/Users/a/.bun/bin/codex",
            "/Users/a/.bun/install/global/node_modules/@openai/codex/bin/codex",
            false,
        ),
        ("/usr/local/bin/codex", "/usr/local/bin/codex", false),
    ];
    for (bin, real, expected) in cases {
        let (source, _) = classify_source(ToolId::Codex, Path::new(bin), Path::new(real));
        let ours = source == InstallSource::NodeManagerNpm;
        assert_eq!(ours, expected, "{bin}");
    }
}

#[test]
fn uninstall_uses_the_package_manager_that_owns_the_install() {
    let cases = [
        (
            InstallSource::NodeManagerNpm,
            "/Users/a/.nvm/versions/node/v20/bin/codex",
            AllowedProgram::Npm,
            vec!["uninstall", "-g", "@openai/codex"],
        ),
        (
            InstallSource::Volta,
            "/Users/a/.volta/bin/codex",
            AllowedProgram::Volta,
            vec!["uninstall", "@openai/codex"],
        ),
        (
            InstallSource::Bun,
            "/Users/a/.bun/bin/codex",
            AllowedProgram::Bun,
            vec!["remove", "-g", "@openai/codex"],
        ),
        (
            InstallSource::Pnpm,
            "/Users/a/Library/pnpm/codex",
            AllowedProgram::Pnpm,
            vec!["remove", "-g", "@openai/codex"],
        ),
    ];
    for (source, bin, program, args) in cases {
        let probe = probe_with(entry(source, bin, "@openai/codex"));
        let plan = uninstall_plan(ToolId::Codex, &probe).expect("uninstall plan");
        let UninstallPlan::Command(commands) = plan else {
            panic!("{source:?} must uninstall through a command");
        };
        let spec = &commands.attempts[0].steps[0].spec;
        assert_eq!(spec.program, program, "{source:?}");
        assert_eq!(
            spec.args,
            args.iter().map(|a| a.to_string()).collect::<Vec<_>>(),
            "{source:?}"
        );
        assert!(spec.program_path.is_some(), "{source:?} must be anchored");
        // Gap left over from the T6 review: every_generated_spec_passes_validation only covered
        // install/update, and the specs of the uninstall command variants never went through validate().
        spec.validate()
            .unwrap_or_else(|e| panic!("invalid uninstall spec for {source:?}: {e:?}"));
    }
}

#[test]
fn uninstalling_a_native_install_removes_its_files() {
    let probe = probe_with(entry(
        InstallSource::Native,
        "/Users/a/.local/bin/claude",
        "@anthropic-ai/claude-code",
    ));
    let plan = uninstall_plan(ToolId::ClaudeCode, &probe).expect("uninstall plan");
    let UninstallPlan::RemovePaths(paths) = plan else {
        panic!("a native install has no package manager to call");
    };
    assert!(paths.contains(&PathBuf::from("/Users/a/.local/bin/claude")));
    assert!(paths.iter().any(|path| path.ends_with("share/claude")));
}

#[test]
fn uninstalling_a_standalone_codex_install_removes_the_launcher_and_the_standalone_store() {
    let plan = uninstall_plan(ToolId::Codex, &probe_with(standalone_codex()))
        .expect("standalone Codex uninstall plan");
    let UninstallPlan::RemovePaths(paths) = plan else {
        panic!("a standalone install has no package manager to call");
    };
    assert!(paths.contains(&PathBuf::from("/Users/a/.local/bin/codex")));
    assert!(paths
        .iter()
        .any(|path| path.ends_with(".codex/packages/standalone")));
    assert!(
        !paths.iter().any(|path| path.ends_with(".codex")),
        "the settings directory is only removed through the explicit option"
    );
}

#[test]
fn an_unmanaged_or_missing_install_refuses_to_guess() {
    let missing =
        uninstall_plan(ToolId::Codex, &empty_probe()).expect_err("nothing to uninstall must fail");
    assert_eq!(missing.code, ErrorCode::ToolNotFound);
    assert_eq!(missing.message_key, "error.tool.notInstalled");

    let unmanaged = probe_with(entry(
        InstallSource::Unmanaged,
        "/usr/local/bin/codex",
        "@openai/codex",
    ));
    let error = uninstall_plan(ToolId::Codex, &unmanaged)
        .expect_err("an unknown owner must not be guessed at");
    assert_eq!(error.code, ErrorCode::UninstallFailed);
    assert_eq!(error.message_key, "error.tool.uninstallStrategyUnknown");
    assert!(error.remediation.is_some());
}

#[test]
fn the_tool_binary_allowlist_matches_the_upstream_cli_name() {
    use crate::compat::ccswitch::tools::tool_id_to_cli_name;
    for id in ToolId::ALL {
        assert_eq!(
            super::tool_program(id).as_str(),
            tool_id_to_cli_name(id),
            "{} binary name drifted",
            id.as_str()
        );
    }
}
