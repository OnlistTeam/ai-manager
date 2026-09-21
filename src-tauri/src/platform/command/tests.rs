use super::{AllowedProgram, CommandSpec};
use crate::domain::ErrorCode;
use std::time::Duration;

fn args(values: &[&str]) -> Vec<String> {
    values.iter().map(|value| value.to_string()).collect()
}

#[test]
fn allowlist_program_names_are_stable() {
    let pairs = [
        (AllowedProgram::Npm, "npm"),
        (AllowedProgram::Pnpm, "pnpm"),
        (AllowedProgram::Bun, "bun"),
        (AllowedProgram::Brew, "brew"),
        (AllowedProgram::Uv, "uv"),
        (AllowedProgram::Pipx, "pipx"),
        (AllowedProgram::Winget, "winget"),
        (AllowedProgram::Powershell, "powershell"),
        (AllowedProgram::Cmd, "cmd"),
        (AllowedProgram::Bash, "bash"),
        (AllowedProgram::Osascript, "osascript"),
        (AllowedProgram::Open, "open"),
        (AllowedProgram::Codesign, "codesign"),
        (AllowedProgram::Wsl, "wsl"),
        (AllowedProgram::Env, "env"),
        (AllowedProgram::XdgTerminalExec, "xdg-terminal-exec"),
        (AllowedProgram::GnomeTerminal, "gnome-terminal"),
        (AllowedProgram::Konsole, "konsole"),
        (AllowedProgram::XTerminalEmulator, "x-terminal-emulator"),
        (AllowedProgram::ChatGptDesktop, "chatgpt"),
        (AllowedProgram::ClaudeDesktop, "claude-desktop"),
    ];
    for (program, expected) in pairs {
        assert_eq!(program.as_str(), expected);
        let json = serde_json::to_string(&program).expect("serialize program");
        assert_eq!(json, format!("\"{expected}\""));
    }
}

#[test]
fn default_spec_is_valid_and_serializes_timeout_as_millis() {
    let spec = CommandSpec::new(
        AllowedProgram::Npm,
        args(&["install", "-g", "@anthropic-ai/claude-code"]),
    );
    assert!(spec.validate().is_ok());
    assert_eq!(spec.timeout, Duration::from_secs(300));
    let json = serde_json::to_string(&spec).expect("serialize spec");
    assert_eq!(
        json,
        r#"{"program":"npm","programPath":null,"args":["install","-g","@anthropic-ai/claude-code"],"env":[],"timeout":300000,"sensitiveArgIndices":[]}"#
    );
    let parsed: CommandSpec = serde_json::from_str(&json).expect("deserialize spec");
    assert_eq!(parsed, spec);
}

#[test]
fn redacted_display_masks_sensitive_args_only() {
    let spec = CommandSpec::new(
        AllowedProgram::Npm,
        args(&["config", "set", "//registry/:_authToken", "sk-secret-value"]),
    )
    .with_sensitive_args(vec![3]);
    assert_eq!(
        spec.redacted_display(),
        "npm config set //registry/:_authToken ***"
    );
    assert!(!spec.redacted_display().contains("sk-secret-value"));
}

#[test]
fn validation_rejects_every_malformed_spec() {
    let out_of_range = CommandSpec::new(AllowedProgram::Brew, args(&["install", "opencode"]))
        .with_sensitive_args(vec![2]);
    let error = out_of_range
        .validate()
        .expect_err("out of range index fails");
    assert_eq!(error.code, ErrorCode::Internal);
    assert_eq!(
        error.message_key,
        "error.commandSpec.sensitiveIndexOutOfRange"
    );

    let with_nul = CommandSpec::new(AllowedProgram::Bash, vec!["-c\0evil".to_string()]);
    assert_eq!(
        with_nul
            .validate()
            .expect_err("interior NUL fails")
            .message_key,
        "error.commandSpec.argContainsNul"
    );

    let no_timeout = CommandSpec::new(AllowedProgram::Bun, args(&["add", "-g", "opencode-ai"]))
        .with_timeout(Duration::from_millis(0));
    assert_eq!(
        no_timeout
            .validate()
            .expect_err("zero timeout fails")
            .message_key,
        "error.commandSpec.timeoutZero"
    );

    let invalid_env_key =
        CommandSpec::new(AllowedProgram::Npm, args(&["--version"])).with_env("BAD=KEY", "value");
    assert_eq!(
        invalid_env_key
            .validate()
            .expect_err("invalid env key fails")
            .message_key,
        "error.commandSpec.envKeyInvalid"
    );

    let env_with_nul = CommandSpec::new(AllowedProgram::Npm, args(&["--version"]))
        .with_env("PATH", "/usr/bin\0/tmp");
    assert_eq!(
        env_with_nul
            .validate()
            .expect_err("env value with NUL fails")
            .message_key,
        "error.commandSpec.envContainsNul"
    );
}

#[test]
fn env_pairs_survive_the_round_trip() {
    let spec = CommandSpec::new(AllowedProgram::Pnpm, args(&["add", "-g", "opencode-ai"]))
        .with_env("PATH", "/usr/local/bin")
        .with_timeout(Duration::from_secs(30));
    assert!(spec.validate().is_ok());
    let json = serde_json::to_string(&spec).expect("serialize spec");
    assert!(json.contains(r#""env":[["PATH","/usr/local/bin"]]"#));
    assert!(json.contains(r#""timeout":30000"#));
}

#[test]
fn tool_binaries_and_volta_are_part_of_the_allowlist() {
    let pairs = [
        (AllowedProgram::Volta, "volta"),
        (AllowedProgram::ClaudeCode, "claude"),
        (AllowedProgram::Codex, "codex"),
        (AllowedProgram::OpenCode, "opencode"),
        (AllowedProgram::GeminiCli, "gemini"),
        (AllowedProgram::GrokBuild, "grok"),
        (AllowedProgram::OpenClaw, "openclaw"),
        (AllowedProgram::Hermes, "hermes"),
        (AllowedProgram::Pi, "pi"),
        (AllowedProgram::KimiCode, "kimi"),
        (AllowedProgram::DeepSeekDsh, "dsh"),
    ];
    for (program, expected) in pairs {
        assert_eq!(program.as_str(), expected);
    }
    assert_eq!(
        serde_json::to_string(&AllowedProgram::ClaudeCode).expect("serialize program"),
        "\"claudeCode\""
    );
    assert_eq!(
        serde_json::to_string(&AllowedProgram::OpenCode).expect("serialize program"),
        "\"openCode\""
    );
}

#[test]
fn only_product_cli_programs_are_terminal_targets() {
    for program in [
        AllowedProgram::ClaudeCode,
        AllowedProgram::Codex,
        AllowedProgram::OpenCode,
        AllowedProgram::GeminiCli,
        AllowedProgram::GrokBuild,
        AllowedProgram::OpenClaw,
        AllowedProgram::Hermes,
        AllowedProgram::Pi,
        AllowedProgram::KimiCode,
        AllowedProgram::DeepSeekDsh,
    ] {
        assert!(program.is_tool());
    }
    for program in [
        AllowedProgram::Npm,
        AllowedProgram::Uv,
        AllowedProgram::Pipx,
        AllowedProgram::Cmd,
        AllowedProgram::Osascript,
        AllowedProgram::Open,
        AllowedProgram::Codesign,
        AllowedProgram::ChatGptDesktop,
        AllowedProgram::ClaudeDesktop,
        AllowedProgram::Wsl,
        AllowedProgram::Env,
        AllowedProgram::XdgTerminalExec,
        AllowedProgram::GnomeTerminal,
        AllowedProgram::Konsole,
        AllowedProgram::XTerminalEmulator,
    ] {
        assert!(!program.is_tool());
    }
}

#[test]
fn an_absolute_program_path_anchors_the_spec_without_leaving_the_allowlist() {
    // `Path::is_absolute` is platform-dependent (Unix accepts `/…`, Windows accepts `C:\…`), so
    // `temp_dir()` is used to build a path that is absolute on both platforms.
    let npm = std::env::temp_dir().join("npm");
    let spec = CommandSpec::new(AllowedProgram::Npm, args(&["install", "-g", "opencode-ai"]))
        .with_program_path(npm.clone());
    assert!(spec.validate().is_ok());
    assert_eq!(spec.program_command_name(), npm.clone().into_os_string());
    assert!(spec
        .redacted_display()
        .starts_with(&npm.display().to_string()));
}

#[test]
fn a_shim_extension_still_matches_the_allowlisted_program() {
    // On Windows npm is `npm.cmd` and volta is `volta.exe`: compare the file_stem, not the full name.
    for (name, program) in [
        ("npm.cmd", AllowedProgram::Npm),
        ("npm.exe", AllowedProgram::Npm),
        ("volta.exe", AllowedProgram::Volta),
    ] {
        let spec = CommandSpec::new(program, args(&["--version"]))
            .with_program_path(std::env::temp_dir().join(name));
        assert!(spec.validate().is_ok(), "{name} must be accepted");
    }
}

#[test]
fn a_program_path_that_is_relative_or_a_different_program_is_rejected() {
    let relative = CommandSpec::new(AllowedProgram::Npm, args(&["install"]))
        .with_program_path(std::path::PathBuf::from("bin/npm"));
    assert_eq!(
        relative
            .validate()
            .expect_err("relative path fails")
            .message_key,
        "error.commandSpec.programPathNotAbsolute"
    );

    let mismatched = CommandSpec::new(AllowedProgram::Npm, args(&["install"]))
        .with_program_path(std::env::temp_dir().join("curl"));
    let error = mismatched
        .validate()
        .expect_err("mismatched basename fails");
    assert_eq!(error.code, ErrorCode::Internal);
    assert_eq!(error.message_key, "error.commandSpec.programPathMismatch");
}

#[test]
fn bash_specs_must_be_a_single_constant_script() {
    let spec = CommandSpec::bash_script("printf hello");
    assert!(spec.validate().is_ok());
    assert_eq!(spec.args, args(&["-c", "printf hello"]));

    // The seal is set to true by hand here (test code is a submodule of the same crate and can touch
    // private fields), purely to test the "shape rule" and the "seal rule" as two independent lines
    // of defence — otherwise both cases would hit bashNotSealed first and bashRequiresSingleScript
    // would never be exercised.
    let mut extra = CommandSpec::new(AllowedProgram::Bash, args(&["-c", "a", "b"]));
    extra.script_sealed = true;
    assert_eq!(
        extra.validate().expect_err("extra args fail").message_key,
        "error.commandSpec.bashRequiresSingleScript"
    );

    let mut login = CommandSpec::new(AllowedProgram::Bash, args(&["-lc", "a"]));
    login.script_sealed = true;
    assert_eq!(
        login.validate().expect_err("non -c flag fails").message_key,
        "error.commandSpec.bashRequiresSingleScript"
    );
}

#[test]
fn an_unsealed_bash_spec_fails_validation_even_with_the_right_shape() {
    // PoC 1: `new(Bash, ...)` can produce the exact `["-c", script]` shape
    // that `bash_script()` also produces, but without going through the
    // one sanctioned constant-script constructor. The seal — not shape —
    // is what must gate this.
    let spec = CommandSpec::new(AllowedProgram::Bash, args(&["-c", "x"]));
    assert_eq!(
        spec.validate()
            .expect_err("unsealed bash spec must fail")
            .message_key,
        "error.commandSpec.scriptNotSealed"
    );
}

#[test]
fn bash_script_output_is_sealed_and_passes_validation() {
    let spec = CommandSpec::bash_script("echo hi");
    assert!(spec.validate().is_ok());
}

#[test]
fn a_bash_spec_does_not_survive_the_wire_as_sealed() {
    // Documents the safe default: `bash_sealed` is `#[serde(skip)]`, so a
    // `Bash` spec that round-trips through serde_json (e.g. arriving over
    // IPC) deserializes with the seal reset to `false` and fails
    // validation, even though it was sealed before serialization.
    let spec = CommandSpec::bash_script("echo hi");
    let json = serde_json::to_string(&spec).expect("serialize spec");
    let parsed: CommandSpec = serde_json::from_str(&json).expect("deserialize spec");
    assert_eq!(
        parsed
            .validate()
            .expect_err("deserialized bash spec is unsealed")
            .message_key,
        "error.commandSpec.scriptNotSealed"
    );
}

/// PowerShell and cmd.exe take a script too; the same seal gates them so a
/// future `format!()` into `-Command` or `/K` cannot pass validation.
#[test]
fn powershell_and_cmd_specs_are_sealed_only_by_their_constant_script_constructors() {
    let unsealed = CommandSpec::new(
        AllowedProgram::Powershell,
        args(&["-NoProfile", "-Command", "x"]),
    );
    assert_eq!(
        unsealed
            .validate()
            .expect_err("unsealed powershell spec must fail")
            .message_key,
        "error.commandSpec.scriptNotSealed"
    );
    let unsealed = CommandSpec::new(AllowedProgram::Cmd, args(&["/K", "x"]));
    assert_eq!(
        unsealed
            .validate()
            .expect_err("unsealed cmd spec must fail")
            .message_key,
        "error.commandSpec.scriptNotSealed"
    );

    let command = CommandSpec::powershell_script("Write-Output hi");
    assert!(command.validate().is_ok());
    assert_eq!(
        command.args,
        args(&[
            "-NoLogo",
            "-NoProfile",
            "-NonInteractive",
            "-Command",
            "Write-Output hi"
        ])
    );

    let encoded = CommandSpec::powershell_encoded_script("echo hi");
    assert!(encoded.validate().is_ok());
    assert_eq!(
        encoded.args,
        args(&[
            "-NoProfile",
            "-ExecutionPolicy",
            "Bypass",
            "-EncodedCommand",
            "ZQBjAGgAbwAgAGgAaQA="
        ])
    );

    let cmd = CommandSpec::cmd_script(r#""%AI_MANAGER_TOOL%""#);
    assert!(cmd.validate().is_ok());
    assert_eq!(
        cmd.args,
        args(&["/D", "/S", "/K", r#""%AI_MANAGER_TOOL%""#])
    );

    // The seal does not survive the wire for these shells either.
    let json = serde_json::to_string(&command).expect("serialize spec");
    let parsed: CommandSpec = serde_json::from_str(&json).expect("deserialize spec");
    assert_eq!(
        parsed
            .validate()
            .expect_err("deserialized powershell spec is unsealed")
            .message_key,
        "error.commandSpec.scriptNotSealed"
    );
}

#[test]
fn a_spec_without_a_program_path_still_reports_the_bare_allowlist_name() {
    let spec = CommandSpec::new(AllowedProgram::ClaudeCode, args(&["update"]));
    assert!(spec.validate().is_ok());
    assert_eq!(
        spec.program_command_name(),
        std::ffi::OsString::from("claude")
    );
    assert_eq!(spec.redacted_display(), "claude update");
}

#[test]
fn env_pairs_can_be_appended_in_bulk() {
    let spec = CommandSpec::new(AllowedProgram::Npm, args(&["install"]))
        .with_env("A", "1")
        .with_env_pairs(vec![("PATH".to_string(), "/usr/bin".to_string())]);
    assert_eq!(
        spec.env,
        vec![
            ("A".to_string(), "1".to_string()),
            ("PATH".to_string(), "/usr/bin".to_string())
        ]
    );
}
