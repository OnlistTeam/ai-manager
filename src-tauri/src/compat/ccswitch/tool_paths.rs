use std::path::{Path, PathBuf};

use crate::compat::ccswitch::install_probe::{InstallSource, InstalledEntry};
use crate::domain::ToolId;

/// The locations spec §32 "Remove Settings" must delete. All taken from existing upstream
/// functions; no paths are assembled here.
///
/// **Known semantic overlap**: claude / codex keep session records **inside** the config
/// directory, so ticking Remove Settings inevitably also removes the cache. That is the
/// real layout of those two tools, not an implementation defect; the UI copy must say so
/// honestly (Phase 3b).
pub fn settings_paths(id: ToolId) -> Vec<PathBuf> {
    match id {
        ToolId::ClaudeCode => vec![
            crate::config::get_claude_config_dir(),
            crate::config::get_default_claude_mcp_path(),
        ],
        ToolId::Codex => vec![crate::codex_config::get_codex_config_dir()],
        ToolId::OpenCode => vec![crate::opencode_config::get_opencode_dir()],
        ToolId::GeminiCli => vec![crate::gemini_config::get_gemini_dir()],
        ToolId::GrokBuild => vec![crate::grok_config::get_grok_config_dir()],
        ToolId::OpenClaw => vec![crate::openclaw_config::get_openclaw_dir()],
        ToolId::Hermes => vec![crate::hermes_config::get_hermes_dir()],
        ToolId::Pi => crate::pi_config::get_pi_agent_dir().into_iter().collect(),
        ToolId::KimiCode => layout_paths(
            kimi_code_home(),
            &[
                "config.toml",
                "tui.toml",
                "AGENTS.md",
                "mcp.json",
                "credentials",
                "skills",
                "plugins",
            ],
        ),
        ToolId::DeepSeekDsh => layout_paths(
            deepseek_dsh_home(),
            &[
                "settings.yaml",
                ".credentials.yaml",
                ".env",
                "cordis.patch.yml",
                "profiles",
                ".anonymous-user-id",
            ],
        ),
    }
}

/// The locations spec §32 "Remove Cache" must delete: session records / local state stores.
/// All three sources are directories the upstream session_manager already reads, not newly
/// guessed paths:
/// `session_manager/providers/claude.rs:18`, `codex.rs:46`,
/// `opencode_config::get_opencode_data_dir`.
pub fn cache_paths(id: ToolId) -> Vec<PathBuf> {
    match id {
        ToolId::ClaudeCode => vec![crate::config::get_claude_config_dir().join("projects")],
        ToolId::Codex => vec![crate::codex_config::get_codex_config_dir().join("sessions")],
        ToolId::OpenCode => vec![crate::opencode_config::get_opencode_data_dir()],
        ToolId::GeminiCli => Vec::new(),
        ToolId::GrokBuild => Vec::new(),
        ToolId::OpenClaw => vec![crate::openclaw_config::get_openclaw_dir().join("agents")],
        ToolId::Hermes => vec![crate::hermes_config::get_hermes_dir().join("sessions")],
        ToolId::Pi => crate::pi_config::get_pi_agent_dir()
            .map(|dir| vec![dir.join("sessions")])
            .unwrap_or_default(),
        ToolId::KimiCode => {
            let helper_suffixes: &[&str] = if cfg!(target_os = "windows") {
                &["bin/rg.exe", "bin/fd.exe"]
            } else {
                &["bin/rg", "bin/fd"]
            };
            let mut suffixes = vec![
                "session_index.jsonl",
                "sessions",
                "logs",
                "updates",
                "user-history",
            ];
            suffixes.extend_from_slice(helper_suffixes);
            layout_paths(kimi_code_home(), &suffixes)
        }
        ToolId::DeepSeekDsh => layout_paths(deepseek_dsh_home(), &["sessions"]),
    }
}

/// "Does this tool store its chat history inside the settings directory?"
///
/// The uninstall copy in spec §32 needs this fact. The truth comes only from
/// `settings_paths` / `cache_paths` — exactly the two lists the uninstall executor deletes,
/// so computing the copy from the same data makes it **impossible** for it to disagree with
/// the real deletion behaviour.
///
/// A single chat-history location inside the settings directory is enough for the warning
/// to hold, hence `any`. An empty `cache_paths` (gemini-cli) therefore naturally yields
/// false; `all` would instead return true for the empty set and attach the warning to a
/// tool that has no session directory at all.
///
/// `Path::starts_with` compares path components, so `~/.claudex` is not mistaken for a
/// child of `~/.claude`.
pub fn sessions_inside_settings(id: ToolId) -> bool {
    let settings = settings_paths(id);
    cache_paths(id)
        .iter()
        .any(|cache| settings.iter().any(|dir| cache.starts_with(dir)))
}

/// Program files installed by an official installer — there is no package manager to call,
/// so they can only be deleted by path. Any non-`Native` source returns an empty list:
/// those installations belong to a package manager and must go through its uninstall command.
pub fn native_app_paths(id: ToolId, entry: &InstalledEntry) -> Vec<PathBuf> {
    if entry.source != InstallSource::Native {
        return Vec::new();
    }
    let home = crate::config::get_home_dir();
    match id {
        ToolId::ClaudeCode => vec![
            entry.bin_path.clone(),
            home.join(".local").join("share").join("claude"),
        ],
        ToolId::OpenCode => vec![entry.bin_path.clone(), home.join(".opencode")],
        // The standalone installer keeps every release under this store; the
        // rest of `.codex` is configuration and sessions behind the explicit
        // uninstall options (ADR-0033).
        ToolId::Codex => vec![
            entry.bin_path.clone(),
            home.join(".codex").join("packages").join("standalone"),
        ],
        // Only the launcher proven by the fresh probe is removed. The rest of
        // `.kimi-code` contains user configuration and sessions controlled by
        // the two explicit uninstall options below.
        ToolId::KimiCode => vec![entry.bin_path.clone()],
        // Grok's official installer maintains both the launcher and the version store;
        // deleting only the launcher would leave half an installation behind. Until there is
        // a verified official uninstall protocol, return empty so the planner fails closed.
        ToolId::GeminiCli
        | ToolId::GrokBuild
        | ToolId::OpenClaw
        | ToolId::Hermes
        | ToolId::Pi
        | ToolId::DeepSeekDsh => Vec::new(),
    }
}

pub(crate) fn kimi_code_home() -> PathBuf {
    configured_tool_home("KIMI_CODE_HOME", ".kimi-code")
}

fn deepseek_dsh_home() -> PathBuf {
    configured_tool_home("DSH_HOME", ".dsh")
}

fn configured_tool_home(variable: &str, default_dir: &str) -> PathBuf {
    let home = crate::config::get_home_dir();
    let Some(raw) = std::env::var_os(variable).filter(|value| !value.is_empty()) else {
        return home.join(default_dir);
    };
    let configured = PathBuf::from(raw);
    if configured.is_absolute() {
        configured
    } else {
        std::env::current_dir()
            .unwrap_or_else(|_| home.clone())
            .join(configured)
    }
}

/// Exact child targets are safe only when a custom root is itself specific.
/// A one-component override such as `~/Documents` is returned as a single
/// target so the final removal boundary can visibly refuse it instead of
/// deleting coincidentally named files inside an unrelated directory.
fn layout_paths(root: PathBuf, suffixes: &[&str]) -> Vec<PathBuf> {
    let home = crate::config::get_home_dir();
    if let Ok(relative) = root.strip_prefix(&home) {
        let mut components = relative.components();
        if components.next().is_some()
            && components.next().is_none()
            && !matches!(relative.to_str(), Some(".kimi-code" | ".dsh"))
        {
            return vec![root];
        }
    }
    suffixes.iter().map(|suffix| root.join(suffix)).collect()
}

/// The final gate before deletion: the target must be **strictly inside** the user home
/// directory.
///
/// Every deletable target of this product (`~/.claude`, `~/.codex`, `~/.config/opencode`,
/// `~/.local/share/claude`, `~/.opencode`, and the cache directories) satisfies this. If a
/// future target ever falls outside home, this rejects it instead of deleting the wrong thing.
///
/// `Path::strip_prefix` is a purely lexical comparison and does not resolve `..`: after
/// stripping the prefix from `home.join("..")` the remaining component is `ParentDir`, which
/// literally still looks "inside home" while actually pointing at home's parent. Therefore
/// **every** remaining component is additionally required to be `Component::Normal` — any
/// `ParentDir`/`RootDir`/`Prefix`/`CurDir` in the remainder is rejected.
pub fn is_removable(path: &Path) -> bool {
    let home = crate::config::get_home_dir();
    is_removable_from_home(path, &home)
}

fn is_removable_from_home(path: &Path, home: &Path) -> bool {
    if home.as_os_str().is_empty() {
        return false;
    }
    let lexically_inside = match path.strip_prefix(home) {
        Ok(relative) => {
            let mut components = relative.components().peekable();
            components.peek().is_some()
                && components.all(|component| matches!(component, std::path::Component::Normal(_)))
        }
        Err(_) => false,
    };
    if !lexically_inside {
        return false;
    }

    let relative = path
        .strip_prefix(home)
        .expect("lexical containment was checked above");
    if !has_safe_removal_specificity(relative) {
        return false;
    }

    // A lexical `$HOME/.config/tool` check is not enough when `.config` is a
    // symlink or Windows junction to somewhere outside the home directory. In
    // that case recursive deletion would resolve the parent first and could
    // erase an unrelated external tree. Resolve the target (or its nearest
    // existing ancestor) and fail closed unless it still lives under the real
    // home. A final-component symlink is checked via its parent because the
    // remover unlinks that symlink itself and never follows its target.
    canonical_removal_boundary_is_safe(path, home)
}

fn canonical_removal_boundary_is_safe(path: &Path, home: &Path) -> bool {
    let Ok(canonical_home) = std::fs::canonicalize(home) else {
        return false;
    };

    let mut candidate = match std::fs::symlink_metadata(path) {
        Ok(metadata) if metadata.file_type().is_symlink() => match path.parent() {
            Some(parent) => parent,
            None => return false,
        },
        Ok(_) => path,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => path,
        Err(_) => return false,
    };

    loop {
        match std::fs::symlink_metadata(candidate) {
            Ok(_) => break,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                let Some(parent) = candidate.parent() else {
                    return false;
                };
                candidate = parent;
            }
            Err(_) => return false,
        }
    }

    std::fs::canonicalize(candidate)
        .map(|resolved| resolved.starts_with(canonical_home))
        .unwrap_or(false)
}

/// Refuse a broad first-level home directory even though it is technically
/// inside `$HOME`. Configuration overrides can otherwise turn a tool target
/// into `~/Documents`, `~/.config`, or another unrelated user-data root.
/// Known one-component tool roots are safe; every other target must be more
/// specific than a direct child of the home directory. Protected custom roots
/// remain visible in the preview and can be handled manually.
fn has_safe_removal_specificity(relative: &Path) -> bool {
    let mut components = relative.components();
    let Some(std::path::Component::Normal(first)) = components.next() else {
        return false;
    };
    if components.next().is_some() {
        return true;
    }

    matches!(
        first.to_str(),
        Some(
            ".claude"
                | ".claude.json"
                | ".codex"
                | ".gemini"
                | ".grok"
                | ".openclaw"
                | ".hermes"
                | ".opencode"
                | ".kimi-code"
                | ".dsh"
        )
    )
}

#[cfg(test)]
mod tests {
    use super::{cache_paths, native_app_paths, settings_paths};
    use crate::compat::ccswitch::install_probe::{InstallSource, InstalledEntry};
    use crate::domain::ToolId;
    use std::path::PathBuf;

    fn entry(source: InstallSource, bin: &str, real: &str) -> InstalledEntry {
        InstalledEntry {
            bin_path: PathBuf::from(bin),
            real_path: PathBuf::from(real),
            source,
            brew_formula: None,
            runnable: true,
            npm_package: None,
            hermes_owner: None,
        }
    }

    #[test]
    fn every_lifecycle_tool_has_a_settings_location() {
        for id in [ToolId::ClaudeCode, ToolId::Codex, ToolId::OpenCode] {
            let paths = settings_paths(id);
            assert!(!paths.is_empty(), "{} needs settings paths", id.as_str());
            assert!(paths.iter().all(|path| path.is_absolute()));
        }
        let claude = settings_paths(ToolId::ClaudeCode);
        assert!(claude.iter().any(|path| path.ends_with(".claude")));
        assert!(claude.iter().any(|path| path.ends_with(".claude.json")));
        assert!(settings_paths(ToolId::Codex)[0].ends_with(".codex"));
        assert!(settings_paths(ToolId::OpenCode)[0].ends_with("opencode"));
    }

    #[test]
    fn cache_locations_are_the_session_stores_upstream_already_reads() {
        assert!(cache_paths(ToolId::ClaudeCode)[0].ends_with("projects"));
        assert!(cache_paths(ToolId::Codex)[0].ends_with("sessions"));
        assert!(cache_paths(ToolId::OpenCode)[0].ends_with("opencode"));
        assert!(cache_paths(ToolId::GeminiCli).is_empty());
        assert!(cache_paths(ToolId::KimiCode)
            .iter()
            .any(|path| path.ends_with(".kimi-code/sessions")));
        assert!(cache_paths(ToolId::DeepSeekDsh)[0].ends_with(".dsh/sessions"));
    }

    #[test]
    #[serial_test::serial]
    fn kimi_and_dsh_uninstall_targets_follow_their_documented_layouts() {
        let kimi = settings_paths(ToolId::KimiCode);
        assert!(kimi
            .iter()
            .any(|path| path.ends_with(".kimi-code/config.toml")));
        assert!(kimi
            .iter()
            .any(|path| path.ends_with(".kimi-code/credentials")));
        assert!(kimi.iter().any(|path| path.ends_with(".kimi-code/skills")));

        let dsh = settings_paths(ToolId::DeepSeekDsh);
        assert!(dsh.iter().any(|path| path.ends_with(".dsh/settings.yaml")));
        assert!(dsh
            .iter()
            .any(|path| path.ends_with(".dsh/.credentials.yaml")));
        assert!(dsh.iter().any(|path| path.ends_with(".dsh/profiles")));
    }

    #[test]
    #[serial_test::serial]
    fn broad_custom_tool_homes_are_visible_but_never_auto_removed() {
        let old = std::env::var_os("KIMI_CODE_HOME");
        let broad = crate::config::get_home_dir().join("Documents");
        std::env::set_var("KIMI_CODE_HOME", &broad);
        let paths = settings_paths(ToolId::KimiCode);
        match old {
            Some(value) => std::env::set_var("KIMI_CODE_HOME", value),
            None => std::env::remove_var("KIMI_CODE_HOME"),
        }
        assert_eq!(paths, vec![broad.clone()]);
        assert!(!super::is_removable(&broad));
    }

    /// The uninstall copy in spec §32 must state honestly whether "ticking Remove Settings
    /// also deletes the chat history". The truth has to be computed, not hardcoded per tool
    /// name: `get_opencode_data_dir()` reads `XDG_DATA_HOME`, and a hardcoded table would lie
    /// once the user changes their environment.
    ///
    /// `get_claude_config_dir()` also reads the process-wide settings cache.
    /// Provider/config tests temporarily reload that cache with custom paths,
    /// so this truth check must share their serial-test lock.
    #[test]
    #[serial_test::serial]
    fn only_the_tools_that_really_nest_sessions_inside_settings_say_so() {
        use super::sessions_inside_settings;

        // claude's `~/.claude/projects` and codex's `~/.codex/sessions` both live inside
        // their respective config directories; opencode's data directory sits under
        // `~/.local/share`, separate from `~/.config/opencode`; gemini-cli has no session
        // directory at all.
        assert!(sessions_inside_settings(ToolId::ClaudeCode));
        assert!(sessions_inside_settings(ToolId::Codex));
        assert!(!sessions_inside_settings(ToolId::OpenCode));
        assert!(!sessions_inside_settings(ToolId::GeminiCli));
        assert!(!sessions_inside_settings(ToolId::KimiCode));
        assert!(!sessions_inside_settings(ToolId::DeepSeekDsh));
    }

    #[test]
    fn native_app_paths_cover_the_launcher_and_the_version_store() {
        let claude = native_app_paths(
            ToolId::ClaudeCode,
            &entry(
                InstallSource::Native,
                "/Users/a/.local/bin/claude",
                "/Users/a/.local/share/claude/versions/2.1.0/claude",
            ),
        );
        assert!(claude.contains(&PathBuf::from("/Users/a/.local/bin/claude")));
        assert!(claude.iter().any(|path| path.ends_with("share/claude")));

        let opencode = native_app_paths(
            ToolId::OpenCode,
            &entry(
                InstallSource::Native,
                "/Users/a/.opencode/bin/opencode",
                "/Users/a/.opencode/bin/opencode",
            ),
        );
        assert!(opencode.iter().any(|path| path.ends_with(".opencode")));

        let kimi = native_app_paths(
            ToolId::KimiCode,
            &entry(
                InstallSource::Native,
                "/Users/a/.kimi-code/bin/kimi",
                "/Users/a/.kimi-code/bin/kimi",
            ),
        );
        assert_eq!(kimi, vec![PathBuf::from("/Users/a/.kimi-code/bin/kimi")]);

        // ADR-0033: the standalone Codex store, never the whole `.codex` home.
        let codex = native_app_paths(
            ToolId::Codex,
            &entry(
                InstallSource::Native,
                "/Users/a/.local/bin/codex",
                "/Users/a/.codex/packages/standalone/releases/0.100.0/codex",
            ),
        );
        assert!(codex.contains(&PathBuf::from("/Users/a/.local/bin/codex")));
        assert!(codex
            .iter()
            .any(|path| path.ends_with(".codex/packages/standalone")));
        assert!(!codex.iter().any(|path| path.ends_with(".codex")));
    }

    #[test]
    fn a_package_managed_install_has_no_native_app_paths() {
        let managed = entry(
            InstallSource::NodeManagerNpm,
            "/Users/a/.nvm/versions/node/v20/bin/claude",
            "/Users/a/.nvm/versions/node/v20/lib/node_modules/@anthropic-ai/claude-code/cli.js",
        );
        assert!(native_app_paths(ToolId::ClaudeCode, &managed).is_empty());
    }

    #[test]
    fn grok_native_install_is_not_partially_removed() {
        let grok = entry(
            InstallSource::Native,
            "/Users/a/.grok/bin/grok",
            "/Users/a/.grok/downloads/grok-0.1.4/grok",
        );
        assert!(native_app_paths(ToolId::GrokBuild, &grok).is_empty());
    }

    #[test]
    #[serial_test::serial]
    fn only_paths_strictly_inside_the_home_directory_may_be_removed() {
        use super::is_removable;
        use std::path::Path;

        let home = crate::config::get_home_dir();
        assert!(is_removable(&home.join(".claude")));
        assert!(is_removable(
            &home.join(".local").join("share").join("claude")
        ));
        assert!(
            !is_removable(&home),
            "the home directory itself is off limits"
        );
        assert!(!is_removable(Path::new("/")));
        assert!(!is_removable(Path::new("/usr/local/bin/claude")));
        assert!(!is_removable(Path::new(".claude")));
    }

    /// `strip_prefix` is a lexical comparison and does not resolve `..`: without the
    /// `Component::Normal` check, `home.join("..")` would be mistaken for "inside home" and
    /// deletions outside home would be allowed through.
    #[test]
    #[serial_test::serial]
    fn parent_dir_segments_after_the_home_prefix_are_refused() {
        use super::is_removable;

        let home = crate::config::get_home_dir();
        assert!(!is_removable(&home.join("..")));
        assert!(!is_removable(&home.join("..").join("..").join("etc")));
        assert!(!is_removable(&home.join(".claude").join("..").join("..")));
        // A mix with normal components is rejected too: anything short of "all Normal" fails.
        assert!(!is_removable(&home.join(".claude").join("..")));
        assert!(
            is_removable(&home.join(".claude")),
            "a genuine child path is still allowed"
        );
    }

    #[test]
    fn broad_or_unknown_home_roots_require_manual_removal() {
        use super::is_removable_from_home;

        let home = tempfile::tempdir().expect("home");
        assert!(!is_removable_from_home(
            &home.path().join("Documents"),
            home.path(),
        ));
        assert!(!is_removable_from_home(
            &home.path().join(".config"),
            home.path(),
        ));
        assert!(!is_removable_from_home(
            &home.path().join("custom-config"),
            home.path(),
        ));
        assert!(is_removable_from_home(
            &home.path().join(".gemini"),
            home.path(),
        ));
        assert!(is_removable_from_home(
            &home.path().join("Documents").join("gemini"),
            home.path(),
        ));
    }

    #[cfg(unix)]
    #[test]
    fn a_parent_symlink_cannot_redirect_deletion_outside_home() {
        use super::is_removable_from_home;
        use std::os::unix::fs::symlink;

        let home = tempfile::tempdir().expect("home");
        let outside = tempfile::tempdir().expect("outside");
        symlink(outside.path(), home.path().join("redirect")).expect("parent symlink");

        assert!(!is_removable_from_home(
            &home.path().join("redirect").join("tool-data"),
            home.path(),
        ));
    }

    #[cfg(unix)]
    #[test]
    fn a_final_symlink_is_safe_because_only_the_link_is_removed() {
        use super::is_removable_from_home;
        use std::os::unix::fs::symlink;

        let home = tempfile::tempdir().expect("home");
        let outside = tempfile::tempdir().expect("outside");
        let link = home.path().join(".claude");
        symlink(outside.path(), &link).expect("target symlink");

        assert!(is_removable_from_home(&link, home.path()));
    }
}
