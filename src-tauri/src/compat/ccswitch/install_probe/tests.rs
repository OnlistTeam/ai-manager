use super::{
    classify_source, inspection_from_installations, inspection_from_join, sibling_program,
    InstallSource, UpdateInspection,
};
use crate::commands::misc::ToolInstallation;
use crate::domain::{ErrorCode, ToolId};
use std::path::{Path, PathBuf};

fn installation(
    path: &str,
    version: &str,
    runnable: bool,
    is_path_default: bool,
) -> ToolInstallation {
    ToolInstallation {
        path: path.to_string(),
        version: Some(version.to_string()),
        runnable,
        error: None,
        source: "test".to_string(),
        is_path_default,
        real: PathBuf::from(path),
    }
}

fn classify(id: ToolId, bin: &str, real: &str) -> InstallSource {
    classify_source(id, Path::new(bin), Path::new(real)).0
}

/// A probe thread that never finished proves nothing. Reading its absence as
/// "no installations" would send the caller down the bare-install path and
/// re-derive capabilities as if the tool were missing.
#[tokio::test]
async fn a_probe_that_panicked_is_an_error_not_an_empty_inspection() {
    let joined = tokio::task::spawn_blocking(|| -> UpdateInspection {
        panic!("simulated enumeration panic sk-ant-secretvalue0123")
    })
    .await;
    let error = inspection_from_join(joined).expect_err("a panicked probe must not default");
    assert_eq!(error.code, ErrorCode::Internal);
    assert_eq!(error.message_key, "error.command.joinFailed");
    let technical = error.technical_message.expect("join detail");
    assert!(technical.contains("did not complete"));
    assert!(!technical.contains("sk-ant-secretvalue0123"));

    let completed = tokio::task::spawn_blocking(UpdateInspection::default).await;
    assert_eq!(
        inspection_from_join(completed).expect("completed probe"),
        UpdateInspection::default()
    );
}

#[test]
fn a_native_installer_layout_wins_over_every_package_manager_rule() {
    // The upstream claude native check from commands/misc.rs:2870 (bin is the launcher, the real binary is in the version directory)
    assert_eq!(
        classify(
            ToolId::ClaudeCode,
            "/Users/a/.local/bin/claude",
            "/Users/a/.local/share/claude/versions/2.1.0/claude"
        ),
        InstallSource::Native
    );
    assert_eq!(
        classify(
            ToolId::ClaudeCode,
            "/Users/a/.local/bin/claude",
            "/Users/a/claude/versions/2.1.0/claude"
        ),
        InstallSource::Native
    );
    assert_eq!(
        classify(
            ToolId::OpenCode,
            "/Users/a/.opencode/bin/opencode",
            "/Users/a/.opencode/bin/opencode"
        ),
        InstallSource::Native
    );
    assert_eq!(
        classify(
            ToolId::KimiCode,
            "/Users/a/.kimi-code/bin/kimi",
            "/Users/a/.kimi-code/bin/kimi"
        ),
        InstallSource::Native
    );
    // The same .opencode layout does not hold for other tools
    assert_ne!(
        classify(
            ToolId::Codex,
            "/Users/a/.opencode/bin/codex",
            "/Users/a/.opencode/bin/codex"
        ),
        InstallSource::Native
    );
}

/// ADR-0033: the standalone Codex installer keeps the real binary under
/// `~/.codex/packages/standalone/`; package-manager copies stay unaffected and
/// the rule never leaks into another tool.
#[test]
fn a_standalone_codex_install_is_native_ownership() {
    assert_eq!(
        classify(
            ToolId::Codex,
            "/Users/a/.local/bin/codex",
            "/Users/a/.codex/packages/standalone/releases/0.100.0/codex"
        ),
        InstallSource::Native
    );
    assert_ne!(
        classify(
            ToolId::Codex,
            "/Users/a/.nvm/versions/node/v22/bin/codex",
            "/Users/a/.nvm/versions/node/v22/lib/node_modules/@openai/codex/bin/codex.js"
        ),
        InstallSource::Native
    );
    assert_ne!(
        classify(
            ToolId::ClaudeCode,
            "/Users/a/.local/bin/claude",
            "/Users/a/.codex/packages/standalone/releases/0.100.0/claude"
        ),
        InstallSource::Native
    );
}

/// The official installer honours `KIMI_CODE_HOME`; search paths and the
/// settings/cache layout already follow it, so ownership must as well.
#[test]
#[serial_test::serial]
fn a_kimi_launcher_under_the_configured_home_is_still_the_native_install() {
    let custom = tempfile::tempdir().expect("custom kimi home");
    let previous = std::env::var_os("KIMI_CODE_HOME");
    std::env::set_var("KIMI_CODE_HOME", custom.path());
    let launcher = custom.path().join("bin").join("kimi");
    let launcher = launcher.to_string_lossy().into_owned();
    let custom_home = classify(ToolId::KimiCode, &launcher, &launcher);
    let default_home = classify(
        ToolId::KimiCode,
        "/Users/a/.kimi-code/bin/kimi",
        "/Users/a/.kimi-code/bin/kimi",
    );
    let sibling_dir = custom.path().join("kimi");
    let sibling_dir = sibling_dir.to_string_lossy().into_owned();
    let outside_bin = classify(ToolId::KimiCode, &sibling_dir, &sibling_dir);
    match previous {
        Some(value) => std::env::set_var("KIMI_CODE_HOME", value),
        None => std::env::remove_var("KIMI_CODE_HOME"),
    }
    assert_eq!(custom_home, InstallSource::Native);
    assert_eq!(default_home, InstallSource::Native);
    assert_ne!(outside_bin, InstallSource::Native);
}

#[test]
fn a_homebrew_cellar_real_target_is_classified_as_brew_with_its_formula() {
    let (source, formula) = classify_source(
        ToolId::GeminiCli,
        Path::new("/opt/homebrew/bin/gemini"),
        Path::new("/opt/homebrew/Cellar/gemini-cli/0.13.0/bin/gemini"),
    );
    assert_eq!(source, InstallSource::Brew);
    assert_eq!(formula.as_deref(), Some("gemini-cli"));
}

/// `brew install --cask claude-code` links `/opt/homebrew/bin/claude` into the
/// Caskroom. That is a Homebrew owner, not the Node manager the generic
/// `/homebrew/` prefix rule would guess; `brew upgrade --cask` is its channel.
#[test]
fn a_homebrew_caskroom_real_target_is_classified_as_a_cask_with_its_token() {
    let (source, token) = classify_source(
        ToolId::ClaudeCode,
        Path::new("/opt/homebrew/bin/claude"),
        Path::new("/opt/homebrew/Caskroom/claude-code/2.1.236/claude"),
    );
    assert_eq!(source, InstallSource::BrewCask);
    assert_eq!(token.as_deref(), Some("claude-code"));

    // The Intel prefix and a nested artifact directory resolve the same token.
    let (source, token) = classify_source(
        ToolId::Codex,
        Path::new("/usr/local/bin/codex"),
        Path::new("/usr/local/Caskroom/codex/0.5.0/codex-x86_64/codex"),
    );
    assert_eq!(source, InstallSource::BrewCask);
    assert_eq!(token.as_deref(), Some("codex"));
}

#[test]
fn node_manager_layouts_map_to_their_own_package_manager() {
    let cases = [
        (
            "/Users/a/.nvm/versions/node/v20.0.0/bin/codex",
            InstallSource::NodeManagerNpm,
        ),
        (
            "/Users/a/.local/share/fnm_multishells/1/bin/codex",
            InstallSource::NodeManagerNpm,
        ),
        (
            "/Users/a/.local/share/mise/installs/node/20/bin/codex",
            InstallSource::NodeManagerNpm,
        ),
        ("/Users/a/.volta/bin/codex", InstallSource::Volta),
        ("/Users/a/.bun/bin/codex", InstallSource::Bun),
        ("/Users/a/.local/share/pnpm/codex", InstallSource::Pnpm),
        ("/usr/local/bin/codex", InstallSource::Unmanaged),
    ];
    for (bin, expected) in cases {
        assert_eq!(classify(ToolId::Codex, bin, bin), expected, "{bin}");
    }
}

/// Windows layouts (`%APPDATA%\npm`, nvm-windows, Scoop shims) carry no
/// POSIX manager prefix; the npm beside the launcher proves ownership.
#[test]
fn a_windows_launcher_beside_npm_cmd_is_owned_by_that_npm() {
    let dir = tempfile::tempdir().expect("fake Roaming\\npm");
    let codex = dir.path().join("codex.cmd");
    std::fs::write(&codex, b"@echo off").expect("launcher");
    assert_eq!(
        super::windows_node_manager_source(&codex),
        InstallSource::Unmanaged
    );
    std::fs::write(dir.path().join("npm.cmd"), b"@echo off").expect("sibling npm");
    assert_eq!(
        super::windows_node_manager_source(&codex),
        InstallSource::NodeManagerNpm
    );
    assert_eq!(
        super::windows_node_manager_source(Path::new("codex.cmd")),
        InstallSource::Unmanaged
    );
}

#[test]
fn a_sibling_program_is_derived_from_the_entry_directory() {
    assert_eq!(
        sibling_program(
            Path::new("/Users/a/.nvm/versions/node/v20/bin/codex"),
            "npm"
        ),
        Some(PathBuf::from("/Users/a/.nvm/versions/node/v20/bin/npm"))
    );
    // No parent directory -> None, so the caller degrades to not anchoring (the same semantics as the upstream sibling_bin)
    assert_eq!(sibling_program(Path::new("codex"), "npm"), None);
}

#[test]
fn update_inspection_keeps_all_installs_and_the_selected_default() {
    let inspection = inspection_from_installations(
        ToolId::ClaudeCode,
        vec![
            installation("/Users/a/.local/bin/claude", "2.1.211", true, true),
            installation("/opt/homebrew/bin/claude", "2.1.210", true, false),
        ],
        None,
    );
    assert_eq!(inspection.installations.len(), 2);
    assert_eq!(
        inspection
            .lifecycle
            .entry
            .as_ref()
            .map(|entry| entry.bin_path.as_path()),
        Some(Path::new("/Users/a/.local/bin/claude"))
    );
    assert!(inspection.installations[0].is_default);
    assert!(!inspection.installations[1].is_default);
}

#[test]
fn update_inspection_refuses_multiple_installs_without_a_default() {
    let inspection = inspection_from_installations(
        ToolId::ClaudeCode,
        vec![
            installation("/a/claude", "2.1.211", true, false),
            installation("/b/claude", "2.1.210", true, false),
        ],
        None,
    );
    assert!(inspection.lifecycle.entry.is_none());
    assert_eq!(inspection.installations.len(), 2);
    assert!(inspection
        .installations
        .iter()
        .all(|installation| !installation.is_default));
}

/// Real-machine probe: asserts no concrete result (it depends on what the dev machine has installed), only the invariants.
#[tokio::test]
async fn probing_a_tool_never_panics_and_keeps_its_invariants() {
    let probe = super::probe(ToolId::ClaudeCode)
        .await
        .expect("a completed probe is never an error");
    if let Some((key, value)) = &probe.path_env {
        assert_eq!(key, "PATH");
        assert!(!value.is_empty());
    }
    if let Some(entry) = &probe.entry {
        assert!(entry.bin_path.is_absolute());
        assert_eq!(entry.npm_package, Some("@anthropic-ai/claude-code"));
    }
}
