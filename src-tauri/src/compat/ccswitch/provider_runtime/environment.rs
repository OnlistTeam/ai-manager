//! What is actually in the terminal, from one tool's point of view.
//!
//! The values come from the login shell (`platform::shell_environment`) and the line
//! attribution comes from the `file` entries of the upstream read-only scanner
//! `services/env_checker.rs`. Their relationship: the shell wins — a declaration in a file
//! may sit in a conditional branch that never ran, so files are only treated as a hint when
//! the shell cannot be started. When the shell does start, an rc file is only recorded as the
//! source if the value it declares matches the value the shell actually exported — a mismatch
//! means that line did not take effect (for instance it was overridden by a file `source`d
//! later), so the value is attributed to `Shell` alone.

use std::collections::BTreeMap;
use std::path::PathBuf;

use super::shell_files;
use crate::domain::ToolId;
use crate::platform::{probe_shell_environment, ShellEnvironment, ShellEnvironmentSource};
use crate::services::env_checker::{check_env_conflicts, EnvConflict};

/// The connection variables each tool reads (endpoint / credential), used to decide whether a
/// login shell is needed (see `probes_login_shell` below); OpenCode's variable names come from
/// `{env:VAR}` interpolation in its config and have no fixed table.
pub(crate) fn connection_variables(tool: ToolId) -> &'static [&'static str] {
    match tool {
        ToolId::ClaudeCode => &[
            "ANTHROPIC_BASE_URL",
            "ANTHROPIC_AUTH_TOKEN",
            "ANTHROPIC_API_KEY",
        ],
        ToolId::Codex => &[
            "CODEX_API_KEY",
            "CODEX_ACCESS_TOKEN",
            "OPENAI_API_KEY",
            "OPENAI_BASE_URL",
        ],
        ToolId::GeminiCli => &["GEMINI_API_KEY", "GOOGLE_API_KEY", "GOOGLE_GEMINI_BASE_URL"],
        ToolId::GrokBuild => &["XAI_API_KEY"],
        ToolId::OpenCode
        | ToolId::OpenClaw
        | ToolId::Hermes
        | ToolId::Pi
        | ToolId::KimiCode
        | ToolId::DeepSeekDsh => &[],
    }
}

/// Tools with no connection variables need no login shell — OpenCode is the exception, since Feature B uses it to resolve `{env:VAR}`.
fn probes_login_shell(tool: ToolId) -> bool {
    !connection_variables(tool).is_empty() || tool == ToolId::OpenCode
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum VariableOrigin {
    /// The rc file line the upstream scanner located (the raw `source_path`, e.g. `/Users/x/.zshrc:8`).
    ShellFile(String),
    /// Present in the login shell but not declared in any scanned rc file (files pulled in via `source`, ...).
    Shell,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ResolvedVariable {
    pub name: String,
    /// Used only in memory inside the compatibility layer; never logged, never in technical_message, never across IPC.
    pub value: String,
    pub origin: VariableOrigin,
}

pub(crate) struct ToolEnvironment {
    shell: ShellEnvironment,
    declared_in_files: Vec<EnvConflict>,
    /// The home directory whose start-up files are searched for the line
    /// responsible for a value. Held rather than read on demand so tests can
    /// point it at a fixture tree instead of the developer's own dotfiles.
    home: PathBuf,
}

impl ToolEnvironment {
    /// Known limitation: rc-file line attribution comes from the upstream read-only scanner's
    /// per-tool keyword table (`get_keywords_for_app` in `services/env_checker.rs`: claude is
    /// `ANTHROPIC*`, codex is `OPENAI*`, gemini is `GEMINI*`/`GOOGLE_GEMINI*`, grokbuild is
    /// `XAI_API_KEY`). `CODEX_API_KEY`, `CODEX_ACCESS_TOKEN` and `GOOGLE_API_KEY` therefore
    /// never match that table and can only show up through the login shell (with origin
    /// `Shell`); once the login shell fails to start, those variables cannot be detected at
    /// all. This is the trade-off ADR-0035 settled on: upstream files stay untouched, the
    /// shell's value wins, and file scan results are only a hint.
    pub(crate) fn detect(tool: ToolId, app_type_name: &str) -> Result<Self, String> {
        let conflicts = check_env_conflicts(app_type_name)?;
        let shell = if probes_login_shell(tool) {
            probe_shell_environment()
        } else {
            // Nothing to look up for this tool: return an empty, "probed" environment — no
            // "could not read the terminal environment" degradation hint, since looking would
            // not turn up anything anyway.
            ShellEnvironment::new(BTreeMap::new(), ShellEnvironmentSource::LoginShell)
        };
        Ok(Self::from_parts_in(
            shell,
            conflicts,
            crate::config::get_home_dir(),
        ))
    }

    /// Without a home directory the start-up tree is not searched at all, so
    /// attribution falls back to the upstream scanner's results. Tests that
    /// only care about that fallback use this; tests about the search itself
    /// pass a fixture tree to `from_parts_in`.
    #[cfg(test)]
    pub(crate) fn from_parts(shell: ShellEnvironment, conflicts: Vec<EnvConflict>) -> Self {
        Self::from_parts_in(shell, conflicts, PathBuf::new())
    }

    pub(crate) fn from_parts_in(
        shell: ShellEnvironment,
        conflicts: Vec<EnvConflict>,
        home: PathBuf,
    ) -> Self {
        Self {
            shell,
            declared_in_files: conflicts
                .into_iter()
                .filter(|conflict| conflict.source_type == "file")
                .collect(),
            home,
        }
    }

    pub(crate) fn shell_inspected(&self) -> bool {
        self.shell.inspected_login_shell()
    }

    /// The start-up line responsible for a value the shell reports, following
    /// `source` so a variable kept in a sourced secrets file is attributed to
    /// that file rather than reported as coming from nowhere in particular.
    fn responsible_line(&self, name: &str, value: &str) -> Option<String> {
        if self.home.as_os_str().is_empty() {
            return None;
        }
        shell_files::responsible_site(&self.home, name, value)
            .map(|site| format!("{}:{}", site.path.display(), site.line))
    }

    pub(crate) fn lookup(&self, name: &str) -> Option<ResolvedVariable> {
        if let Some(value) = self.shell.get(name) {
            let origin = self
                .responsible_line(name, value)
                .or_else(|| {
                    // Anything the search above does not cover: the upstream
                    // scanner reads a few files this one does not follow into.
                    self.declared_in_files
                        .iter()
                        .find(|conflict| {
                            conflict.var_name.eq_ignore_ascii_case(name)
                                && conflict.var_value.trim() == value
                        })
                        .map(|conflict| conflict.source_path.clone())
                })
                .map(VariableOrigin::ShellFile)
                .unwrap_or(VariableOrigin::Shell);
            return Some(ResolvedVariable {
                name: name.to_string(),
                value: value.to_string(),
                origin,
            });
        }
        if self.shell.inspected_login_shell() {
            return None;
        }
        let declared = self
            .declared_in_files
            .iter()
            .find(|conflict| conflict.var_name.eq_ignore_ascii_case(name));
        let conflict = declared.filter(|conflict| !conflict.var_value.trim().is_empty())?;
        Some(ResolvedVariable {
            name: name.to_string(),
            value: conflict.var_value.trim().to_string(),
            origin: VariableOrigin::ShellFile(conflict.source_path.clone()),
        })
    }
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use super::{connection_variables, probes_login_shell, ToolEnvironment, VariableOrigin};
    use crate::domain::ToolId;
    use crate::platform::{ShellEnvironment, ShellEnvironmentSource};
    use crate::services::env_checker::EnvConflict;

    fn shell(pairs: &[(&str, &str)], source: ShellEnvironmentSource) -> ShellEnvironment {
        ShellEnvironment::new(
            pairs
                .iter()
                .map(|(k, v)| (k.to_string(), v.to_string()))
                .collect::<BTreeMap<_, _>>(),
            source,
        )
    }

    fn file(name: &str, value: &str, path: &str) -> EnvConflict {
        EnvConflict {
            var_name: name.to_string(),
            var_value: value.to_string(),
            source_type: "file".to_string(),
            source_path: path.to_string(),
        }
    }

    #[test]
    fn every_provider_tool_declares_its_connection_variables() {
        assert!(connection_variables(ToolId::ClaudeCode).contains(&"ANTHROPIC_BASE_URL"));
        assert!(connection_variables(ToolId::Codex).contains(&"CODEX_API_KEY"));
        assert!(connection_variables(ToolId::GeminiCli).contains(&"GOOGLE_API_KEY"));
        assert!(connection_variables(ToolId::GrokBuild).contains(&"XAI_API_KEY"));
        assert!(connection_variables(ToolId::OpenCode).is_empty());
    }

    #[test]
    fn a_login_shell_value_is_attributed_to_the_file_that_declares_it() {
        let environment = ToolEnvironment::from_parts(
            shell(
                &[("ANTHROPIC_BASE_URL", "https://relay.example.test")],
                ShellEnvironmentSource::LoginShell,
            ),
            vec![file(
                "ANTHROPIC_BASE_URL",
                "https://relay.example.test",
                "/Users/x/.zshrc:8",
            )],
        );
        let resolved = environment.lookup("ANTHROPIC_BASE_URL").expect("resolved");
        assert_eq!(resolved.value, "https://relay.example.test");
        assert_eq!(
            resolved.origin,
            VariableOrigin::ShellFile("/Users/x/.zshrc:8".to_string())
        );
    }

    #[test]
    fn a_sourced_export_without_a_scanned_declaration_is_still_reported() {
        let environment = ToolEnvironment::from_parts(
            shell(
                &[("ANTHROPIC_BASE_URL", "https://relay.example.test")],
                ShellEnvironmentSource::LoginShell,
            ),
            vec![],
        );
        let resolved = environment.lookup("ANTHROPIC_BASE_URL").expect("resolved");
        assert_eq!(resolved.origin, VariableOrigin::Shell);
        assert!(environment.shell_inspected());
    }

    #[test]
    fn an_inactive_file_declaration_is_ignored_when_the_login_shell_was_inspected() {
        let environment = ToolEnvironment::from_parts(
            shell(&[("PATH", "/bin")], ShellEnvironmentSource::LoginShell),
            vec![file(
                "ANTHROPIC_BASE_URL",
                "https://stale.example.test",
                "/Users/x/.bashrc:3",
            )],
        );
        assert!(environment.lookup("ANTHROPIC_BASE_URL").is_none());
    }

    #[test]
    fn file_declarations_are_the_only_clue_when_the_login_shell_could_not_run() {
        let environment = ToolEnvironment::from_parts(
            shell(&[], ShellEnvironmentSource::ProcessFallback),
            vec![file("ANTHROPIC_AUTH_TOKEN", "sk-file", "/Users/x/.zshrc:9")],
        );
        let resolved = environment
            .lookup("ANTHROPIC_AUTH_TOKEN")
            .expect("resolved");
        assert_eq!(resolved.value, "sk-file");
        assert!(!environment.shell_inspected());
    }

    #[test]
    fn lookup_matches_declarations_case_insensitively() {
        let environment = ToolEnvironment::from_parts(
            shell(
                &[("ANTHROPIC_BASE_URL", "https://relay.example.test")],
                ShellEnvironmentSource::LoginShell,
            ),
            vec![file(
                "anthropic_base_url",
                "https://relay.example.test",
                "/Users/x/.zshrc:4",
            )],
        );
        let resolved = environment.lookup("ANTHROPIC_BASE_URL").expect("resolved");
        assert_eq!(
            resolved.origin,
            VariableOrigin::ShellFile("/Users/x/.zshrc:4".to_string())
        );
    }

    #[test]
    fn a_blank_file_value_is_not_a_clue_when_the_shell_could_not_run() {
        let environment = ToolEnvironment::from_parts(
            shell(&[], ShellEnvironmentSource::ProcessFallback),
            vec![file("ANTHROPIC_API_KEY", "   ", "/Users/x/.zshrc:5")],
        );
        assert!(environment.lookup("ANTHROPIC_API_KEY").is_none());
    }

    #[test]
    fn system_entries_from_the_scanner_are_not_file_declarations() {
        let environment = ToolEnvironment::from_parts(
            shell(&[], ShellEnvironmentSource::ProcessFallback),
            vec![EnvConflict {
                var_name: "ANTHROPIC_BASE_URL".to_string(),
                var_value: "https://x.example.test".to_string(),
                source_type: "system".to_string(),
                source_path: "Process Environment".to_string(),
            }],
        );
        assert!(environment.lookup("ANTHROPIC_BASE_URL").is_none());
    }

    #[test]
    fn a_declaration_with_a_different_value_is_not_credited() {
        let environment = ToolEnvironment::from_parts(
            shell(
                &[("ANTHROPIC_BASE_URL", "https://live.example.test")],
                ShellEnvironmentSource::LoginShell,
            ),
            vec![
                file(
                    "ANTHROPIC_BASE_URL",
                    "https://old.example.test",
                    "/Users/x/.bashrc:3",
                ),
                file(
                    "ANTHROPIC_BASE_URL",
                    "https://live.example.test",
                    "/Users/x/.zshrc:140",
                ),
            ],
        );
        let resolved = environment.lookup("ANTHROPIC_BASE_URL").expect("resolved");
        assert_eq!(
            resolved.origin,
            VariableOrigin::ShellFile("/Users/x/.zshrc:140".to_string())
        );
    }

    #[test]
    fn a_lone_stale_declaration_falls_back_to_shell_origin() {
        let environment = ToolEnvironment::from_parts(
            shell(
                &[("ANTHROPIC_BASE_URL", "https://live.example.test")],
                ShellEnvironmentSource::LoginShell,
            ),
            vec![file(
                "ANTHROPIC_BASE_URL",
                "https://old.example.test",
                "/Users/x/.bashrc:3",
            )],
        );
        let resolved = environment.lookup("ANTHROPIC_BASE_URL").expect("resolved");
        assert_eq!(resolved.origin, VariableOrigin::Shell);
    }

    #[test]
    fn probes_login_shell_only_for_tools_with_something_to_look_up() {
        for tool in [
            ToolId::OpenClaw,
            ToolId::Hermes,
            ToolId::Pi,
            ToolId::KimiCode,
            ToolId::DeepSeekDsh,
        ] {
            assert!(!probes_login_shell(tool), "{tool:?}");
        }
        for tool in [
            ToolId::ClaudeCode,
            ToolId::Codex,
            ToolId::OpenCode,
            ToolId::GeminiCli,
            ToolId::GrokBuild,
        ] {
            assert!(probes_login_shell(tool), "{tool:?}");
        }
    }
}
