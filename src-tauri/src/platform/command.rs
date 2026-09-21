use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use std::time::Duration;

use super::redact::REDACTED_PLACEHOLDER;
use crate::domain::{AppError, ErrorCode};

const DEFAULT_TIMEOUT: Duration = Duration::from_secs(300);

/// Spec §14 allowlist: only these programs may be started by the executor.
///
/// The list in §14 says "for example, only allow", so it is an example and not a closed
/// set. This product extends it with two groups:
/// - `Volta`: volta is its own branch in the POSIX anchoring decision tree (upstream
///   `commands/misc.rs:2815`).
/// - The four tool binaries: the executable of the official self-update commands
///   `<bin> update` / `<bin> upgrade` is the tool itself (upstream
///   `commands/misc.rs:2656`), and that path cannot be expressed without listing them.
///   They are written as literals instead of calling
///   `compat::ccswitch::tools::tool_id_to_app_type`: the platform layer must not depend
///   on upstream, and the facade's comparison test pins the two down as consistent.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum AllowedProgram {
    Npm,
    Pnpm,
    Bun,
    Brew,
    Volta,
    Uv,
    Pipx,
    Winget,
    Powershell,
    Cmd,
    Bash,
    Osascript,
    Open,
    /// ADR-0033: only `/usr/bin/codesign` may verify the signature of a native program supplied into the official layout.
    Codesign,
    Wsl,
    Env,
    #[serde(rename = "xdg-terminal-exec")]
    XdgTerminalExec,
    #[serde(rename = "gnome-terminal")]
    GnomeTerminal,
    Konsole,
    #[serde(rename = "x-terminal-emulator")]
    XTerminalEmulator,
    #[serde(rename = "chatgpt")]
    ChatGptDesktop,
    #[serde(rename = "claude-desktop")]
    ClaudeDesktop,
    ClaudeCode,
    Codex,
    OpenCode,
    GeminiCli,
    GrokBuild,
    OpenClaw,
    Hermes,
    Pi,
    KimiCode,
    DeepSeekDsh,
}

impl AllowedProgram {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Npm => "npm",
            Self::Pnpm => "pnpm",
            Self::Bun => "bun",
            Self::Brew => "brew",
            Self::Volta => "volta",
            Self::Uv => "uv",
            Self::Pipx => "pipx",
            Self::Winget => "winget",
            Self::Powershell => "powershell",
            Self::Cmd => "cmd",
            Self::Bash => "bash",
            Self::Osascript => "osascript",
            Self::Open => "open",
            Self::Codesign => "codesign",
            Self::Wsl => "wsl",
            Self::Env => "env",
            Self::XdgTerminalExec => "xdg-terminal-exec",
            Self::GnomeTerminal => "gnome-terminal",
            Self::Konsole => "konsole",
            Self::XTerminalEmulator => "x-terminal-emulator",
            Self::ChatGptDesktop => "chatgpt",
            Self::ClaudeDesktop => "claude-desktop",
            Self::ClaudeCode => "claude",
            Self::Codex => "codex",
            Self::OpenCode => "opencode",
            Self::GeminiCli => "gemini",
            Self::GrokBuild => "grok",
            Self::OpenClaw => "openclaw",
            Self::Hermes => "hermes",
            Self::Pi => "pi",
            Self::KimiCode => "kimi",
            Self::DeepSeekDsh => "dsh",
        }
    }

    /// Only the eight interactive CLIs in the product registry may be terminal launch targets.
    pub fn is_tool(&self) -> bool {
        matches!(
            self,
            Self::ClaudeCode
                | Self::Codex
                | Self::OpenCode
                | Self::GeminiCli
                | Self::GrokBuild
                | Self::OpenClaw
                | Self::Hermes
                | Self::Pi
                | Self::KimiCode
                | Self::DeepSeekDsh
        )
    }

    /// These installers pull packages from the npm registry and understand `NPM_CONFIG_REGISTRY`.
    /// Brew and the tools' official self-updates are excluded, so a community mirror is
    /// never presented as if it applied to every download.
    pub fn uses_npm_registry(&self) -> bool {
        matches!(self, Self::Npm | Self::Pnpm | Self::Bun | Self::Volta)
    }
}

mod duration_millis {
    use serde::{Deserialize, Deserializer, Serialize, Serializer};
    use std::time::Duration;

    pub fn serialize<S: Serializer>(value: &Duration, serializer: S) -> Result<S::Ok, S::Error> {
        u64::try_from(value.as_millis())
            .unwrap_or(u64::MAX)
            .serialize(serializer)
    }

    pub fn deserialize<'de, D: Deserializer<'de>>(deserializer: D) -> Result<Duration, D::Error> {
        Ok(Duration::from_millis(u64::deserialize(deserializer)?))
    }
}

/// Spec §14: commands always take array arguments; string concatenation is forbidden.
///
/// `program` is the **semantic gate** (which class of program may run) and `program_path`
/// is the **anchor** (which one on disk). Upstream `commands/misc.rs:2841-2849` explains
/// it: the PATH of a GUI process comes from launchd / SCM and usually excludes
/// `~/.local/bin`, `/opt/homebrew/bin` and `~/.volta/bin`, so a bare-name invocation is
/// likely to exit 127, while the probe phase uses a login shell — the two PATHs are
/// asymmetric. Every command that means "an installation was found somewhere and must be
/// written back to the same place" has to carry `program_path`.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct CommandSpec {
    pub(crate) program: AllowedProgram,
    pub program_path: Option<PathBuf>,
    pub(crate) args: Vec<String>,
    pub env: Vec<(String, String)>,
    #[serde(with = "duration_millis")]
    pub timeout: Duration,
    pub sensitive_arg_indices: Vec<usize>,
    /// Provenance seal for shell specs (`Bash` / `Powershell` / `Cmd`): only
    /// [`CommandSpec::bash_script`], [`CommandSpec::powershell_script`],
    /// [`CommandSpec::powershell_encoded_script`] and [`CommandSpec::cmd_script`] can set
    /// it to true, and they all accept compile-time constant scripts only.
    ///
    /// `#[serde(skip)]` is deliberate — the wire format has no such field, so any shell
    /// spec that arrives through serde deserialization lands on the default `false`, i.e.
    /// "unsealed". Only then can `validate()` really distinguish a "compile-time constant
    /// script" from one "built at runtime or passed in from outside" — the shape alone
    /// (`["-c", script]`) is not enough, because both `new(Bash, ...)` and mutating `args`
    /// after construction (within the same crate) can forge the same shape.
    #[serde(skip)]
    script_sealed: bool,
}

impl CommandSpec {
    pub fn new(program: AllowedProgram, args: Vec<String>) -> Self {
        Self {
            program,
            program_path: None,
            args,
            env: Vec::new(),
            timeout: DEFAULT_TIMEOUT,
            sensitive_arg_indices: Vec::new(),
            script_sealed: false,
        }
    }

    pub fn with_env(mut self, key: impl Into<String>, value: impl Into<String>) -> Self {
        self.env.push((key.into(), value.into()));
        self
    }

    pub fn with_env_pairs(mut self, pairs: Vec<(String, String)>) -> Self {
        self.env.extend(pairs);
        self
    }

    pub fn with_timeout(mut self, timeout: Duration) -> Self {
        self.timeout = timeout;
        self
    }

    pub fn with_sensitive_args(mut self, indices: Vec<usize>) -> Self {
        self.sensitive_arg_indices = indices;
        self
    }

    pub fn with_program_path(mut self, path: PathBuf) -> Self {
        self.program_path = Some(path);
        self
    }

    /// The only legal entry point for official shell installers.
    ///
    /// Accepts `&'static str` only, and `validate()` requires a `Bash` spec to be exactly
    /// `["-c", <script>]` — the script is a compile-time constant with zero interpolation,
    /// `-c` and the script each occupy one argv element straight into `execve`, and no
    /// second shell layer re-parses anything in between. Any case that needs data inside
    /// the script must be rewritten as "constant script + extra argv arguments (read as
    /// `$1` inside the script)".
    ///
    /// `validate()` decides whether a `Bash` spec is trustworthy from the `script_sealed`
    /// seal, not from its shape.
    pub fn bash_script(script: &'static str) -> Self {
        let mut spec = Self::new(
            AllowedProgram::Bash,
            vec!["-c".to_string(), script.to_string()],
        );
        spec.script_sealed = true;
        spec
    }

    /// The only legal entry point for Windows PowerShell constant scripts (the `-Command`
    /// form). Data always travels through env (read as `$env:…` inside the script) and argv
    /// contains compile-time constants only.
    pub fn powershell_script(script: &'static str) -> Self {
        let mut spec = Self::new(
            AllowedProgram::Powershell,
            vec![
                "-NoLogo".to_string(),
                "-NoProfile".to_string(),
                "-NonInteractive".to_string(),
                "-Command".to_string(),
                script.to_string(),
            ],
        );
        spec.script_sealed = true;
        spec
    }

    /// The only legal entry point for the official PowerShell installer (`irm … | iex`).
    /// The script is still a compile-time constant, only encoded as UTF-16LE base64 per the
    /// PowerShell `-EncodedCommand` convention, which sidesteps quote-escaping ambiguity in
    /// a GUI process.
    pub fn powershell_encoded_script(script: &'static str) -> Self {
        use base64::{engine::general_purpose::STANDARD, Engine as _};

        let mut bytes = Vec::with_capacity(script.len() * 2);
        for unit in script.encode_utf16() {
            bytes.extend_from_slice(&unit.to_le_bytes());
        }
        let mut spec = Self::new(
            AllowedProgram::Powershell,
            vec![
                "-NoProfile".to_string(),
                "-ExecutionPolicy".to_string(),
                "Bypass".to_string(),
                "-EncodedCommand".to_string(),
                STANDARD.encode(bytes),
            ],
        );
        spec.script_sealed = true;
        spec
    }

    /// The only legal entry point for a constant `cmd.exe /K` command line. Data travels through env (`%AI_MANAGER_…%`).
    pub fn cmd_script(script: &'static str) -> Self {
        let mut spec = Self::new(
            AllowedProgram::Cmd,
            vec![
                "/D".to_string(),
                "/S".to_string(),
                "/K".to_string(),
                script.to_string(),
            ],
        );
        spec.script_sealed = true;
        spec
    }

    /// The executable name handed to the process API: the absolute path when anchored, otherwise the bare allowlist name.
    pub fn program_command_name(&self) -> std::ffi::OsString {
        match &self.program_path {
            Some(path) => path.clone().into_os_string(),
            None => std::ffi::OsString::from(self.program.as_str()),
        }
    }

    pub fn validate(&self) -> Result<(), AppError> {
        if self.timeout.is_zero() {
            return Err(AppError::new(
                ErrorCode::Internal,
                "error.commandSpec.timeoutZero",
            ));
        }
        if self.args.iter().any(|arg| arg.contains('\0')) {
            return Err(AppError::new(
                ErrorCode::Internal,
                "error.commandSpec.argContainsNul",
            ));
        }
        if self.env.iter().any(|(key, _)| {
            key.is_empty() || key.contains('=') || key.chars().any(|character| character == '\0')
        }) {
            return Err(AppError::new(
                ErrorCode::Internal,
                "error.commandSpec.envKeyInvalid",
            ));
        }
        if self.env.iter().any(|(_, value)| value.contains('\0')) {
            return Err(AppError::new(
                ErrorCode::Internal,
                "error.commandSpec.envContainsNul",
            ));
        }
        if self
            .sensitive_arg_indices
            .iter()
            .any(|index| *index >= self.args.len())
        {
            return Err(AppError::new(
                ErrorCode::Internal,
                "error.commandSpec.sensitiveIndexOutOfRange",
            ));
        }
        if let Some(path) = &self.program_path {
            if !path.is_absolute() {
                return Err(AppError::new(
                    ErrorCode::Internal,
                    "error.commandSpec.programPathNotAbsolute",
                )
                .with_technical(path.display().to_string()));
            }
            if !program_path_matches(path, self.program) {
                return Err(AppError::new(
                    ErrorCode::Internal,
                    "error.commandSpec.programPathMismatch",
                )
                .with_technical(format!(
                    "{} is not a {}",
                    path.display(),
                    self.program.as_str()
                )));
            }
        }
        // Seal before shape: an unsealed shell spec is always rejected, no matter how much
        // its args look like `["-c", script]` — a shape check can be forged by
        // `new(Bash, ...)` or by mutating `args` after construction, the seal cannot (only
        // the constant-script constructors set it, and it never crosses the wire).
        if matches!(
            self.program,
            AllowedProgram::Bash | AllowedProgram::Powershell | AllowedProgram::Cmd
        ) && !self.script_sealed
        {
            return Err(AppError::new(
                ErrorCode::Internal,
                "error.commandSpec.scriptNotSealed",
            ));
        }
        if self.program == AllowedProgram::Bash && !(self.args.len() == 2 && self.args[0] == "-c") {
            return Err(AppError::new(
                ErrorCode::Internal,
                "error.commandSpec.bashRequiresSingleScript",
            ));
        }
        Ok(())
    }

    /// For logs and UI display: sensitive arguments are replaced with `***` (spec §19, AI_RULES rule 6).
    pub fn redacted_display(&self) -> String {
        let mut parts = Vec::with_capacity(self.args.len() + 1);
        parts.push(match &self.program_path {
            Some(path) => path.display().to_string(),
            None => self.program.as_str().to_string(),
        });
        for (index, arg) in self.args.iter().enumerate() {
            if self.sensitive_arg_indices.contains(&index) {
                parts.push(REDACTED_PLACEHOLDER.to_string());
            } else {
                parts.push(arg.clone());
            }
        }
        parts.join(" ")
    }
}

/// `program_path` must really point at the `program` binary: compare the file stem.
/// On Windows npm is `npm.cmd` and volta is `volta.exe`, so stems are compared rather than
/// full names; the case-insensitive comparison is for Windows and has no side effect on
/// POSIX, where our program names are lowercase anyway.
fn program_path_matches(path: &Path, program: AllowedProgram) -> bool {
    path.file_stem()
        .map(|stem| stem.to_string_lossy().to_ascii_lowercase() == program.as_str())
        .unwrap_or(false)
}

// Split into a subfile because this file plus its full test suite would
// exceed the project's 500-line-per-file limit; see AI_RULES.
#[cfg(test)]
#[path = "command/tests.rs"]
mod tests;
