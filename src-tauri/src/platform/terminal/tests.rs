use super::*;

fn native_spec(program: AllowedProgram, tool_name: &str) -> TerminalLaunchSpec {
    TerminalLaunchSpec::native(
        std::env::temp_dir().join("AI Manager Project"),
        CommandSpec::new(program, Vec::new())
            .with_program_path(std::env::temp_dir().join(tool_name))
            .with_env("PATH", "/opt/homebrew/bin:/usr/bin"),
    )
}

#[test]
fn a_native_target_must_be_an_anchored_product_cli() {
    let valid = native_spec(AllowedProgram::Codex, "codex");
    assert!(valid.validate().is_ok());

    let bare = TerminalLaunchSpec::native(
        std::env::temp_dir(),
        CommandSpec::new(AllowedProgram::Codex, Vec::new()),
    );
    assert_eq!(
        bare.validate()
            .expect_err("bare tool is refused")
            .message_key,
        "error.commandSpec.terminalTargetNotAnchored"
    );

    let package_manager = TerminalLaunchSpec::native(
        std::env::temp_dir(),
        CommandSpec::new(AllowedProgram::Npm, Vec::new())
            .with_program_path(std::env::temp_dir().join("npm")),
    );
    assert_eq!(
        package_manager
            .validate()
            .expect_err("non-tool is refused")
            .message_key,
        "error.commandSpec.terminalTargetInvalid"
    );
}

#[test]
fn terminal_targets_allow_bounded_argv_but_refuse_unbounded_inputs() {
    let with_args = TerminalLaunchSpec::native(
        std::env::temp_dir(),
        CommandSpec::new(
            AllowedProgram::ClaudeCode,
            vec!["--resume".to_string(), "session-42".to_string()],
        )
        .with_program_path(std::env::temp_dir().join("claude")),
    );
    assert!(with_args.validate().is_ok());

    let too_many = TerminalLaunchSpec::native(
        std::env::temp_dir(),
        CommandSpec::new(AllowedProgram::ClaudeCode, vec!["x".to_string(); 17])
            .with_program_path(std::env::temp_dir().join("claude")),
    );
    assert!(too_many.validate().is_err());

    let too_long = TerminalLaunchSpec::native(
        std::env::temp_dir(),
        CommandSpec::new(AllowedProgram::ClaudeCode, vec!["x".repeat(4097)])
            .with_program_path(std::env::temp_dir().join("claude")),
    );
    assert!(too_long.validate().is_err());
}

#[test]
fn terminal_targets_refuse_secrets_and_relative_directories() {
    let with_secret_env = TerminalLaunchSpec::native(
        std::env::temp_dir(),
        CommandSpec::new(AllowedProgram::ClaudeCode, Vec::new())
            .with_program_path(std::env::temp_dir().join("claude"))
            .with_env("API_KEY", "secret"),
    );
    assert!(with_secret_env.validate().is_err());

    let relative = TerminalLaunchSpec::native(
        PathBuf::from("project"),
        CommandSpec::new(AllowedProgram::ClaudeCode, Vec::new())
            .with_program_path(std::env::temp_dir().join("claude")),
    );
    assert_eq!(
        relative
            .validate()
            .expect_err("relative cwd is refused")
            .message_key,
        "error.commandSpec.terminalWorkingDirectoryNotAbsolute"
    );
}

#[test]
fn macos_bridge_keeps_every_dynamic_value_out_of_script_source_and_logs() {
    let mut spec = native_spec(AllowedProgram::Codex, "codex");
    spec.command.args = vec!["resume".to_string(), "session-42".to_string()];
    let bridge = macos_bridge_spec(&spec).expect("bridge plan");
    assert_eq!(bridge.program, AllowedProgram::Osascript);
    assert_eq!(
        bridge.program_path,
        Some(PathBuf::from("/usr/bin/osascript"))
    );
    assert_eq!(bridge.args[0], "-e");
    assert_eq!(bridge.args[1], MACOS_TERMINAL_SCRIPT);
    assert!(!bridge.args[1].contains("AI Manager Project"));
    assert!(!bridge.args[1].contains("/opt/homebrew"));
    assert_eq!(bridge.args[2], spec.working_directory.to_string_lossy());
    assert_eq!(
        bridge.args[3],
        spec.command.program_path.unwrap().to_string_lossy()
    );
    assert_eq!(bridge.args[4], "/opt/homebrew/bin:/usr/bin");
    assert_eq!(&bridge.args[5..], ["resume", "session-42"]);
    assert_eq!(
        bridge.redacted_display(),
        "/usr/bin/osascript -e *** *** *** *** *** ***"
    );
    // The bridge anchors `/usr/bin/osascript`, which only counts as absolute where POSIX
    // paths are; the macOS bridge is never planned on Windows, it is only compiled there.
    if cfg!(unix) {
        assert!(bridge.validate().is_ok());
    }
}

#[test]
fn windows_native_plan_has_fixed_command_source_and_paths_stay_data() {
    let spec = native_spec(AllowedProgram::ClaudeCode, "claude.cmd");
    let plan = windows_native_plan(&spec).expect("native plan");
    assert_eq!(plan.command.program, AllowedProgram::Cmd);
    // `cmd /S` removes one pair of quotes before it resolves anything, so the
    // script carries two. With one pair the tool path would arrive unquoted
    // and a path containing a space would run its first word instead.
    assert_eq!(
        plan.command.args,
        vec!["/D", "/S", "/K", r#"""%AI_MANAGER_TOOL%"""#]
    );
    assert_eq!(
        plan.command.env,
        vec![(
            "AI_MANAGER_TOOL".to_string(),
            spec.command
                .program_path
                .unwrap()
                .to_string_lossy()
                .into_owned()
        )]
    );
    assert_eq!(plan.working_directory, Some(spec.working_directory));
    assert!(plan.command.validate().is_ok());
}

// `\\?\C:\...` only counts as absolute on Windows, and the spec is validated
// before the plan is built, so anywhere else this case is rejected before it
// reaches the code it is about. The normalisation itself is covered
// cross-platform by `platform::tests`.
#[cfg(windows)]
#[test]
fn a_verbatim_tool_path_reaches_cmd_in_a_form_it_can_run() {
    // `cmd` reports "The system cannot find the path specified" for a `\\?\`
    // path, and the probe hands back canonical paths that carry that prefix.
    let mut spec = native_spec(AllowedProgram::ClaudeCode, "claude.cmd");
    spec.command.program_path = Some(PathBuf::from(r"\\?\C:\Program Files\tools\claude.cmd"));

    let plan = windows_native_plan(&spec).expect("native plan");

    assert_eq!(
        plan.command.env,
        vec![(
            "AI_MANAGER_TOOL".to_string(),
            r"C:\Program Files\tools\claude.cmd".to_string()
        )]
    );
}

#[test]
fn windows_session_resume_uses_direct_argv_instead_of_a_shell_string() {
    let mut spec = native_spec(AllowedProgram::Codex, "codex.exe");
    spec.command.args = vec!["resume".to_string(), "session with spaces".to_string()];
    let plan = windows_native_plan(&spec).expect("native session plan");
    assert_eq!(plan.command.program, AllowedProgram::Codex);
    assert_eq!(plan.command.program_path, spec.command.program_path);
    assert_eq!(plan.command.args, spec.command.args);
    assert_eq!(plan.working_directory, Some(spec.working_directory));
}

#[test]
fn wsl_unc_paths_are_bound_to_the_configured_distribution() {
    let same = Path::new(r"\\wsl.localhost\Ubuntu-24.04\home\one\project");
    assert!(matches!(
        classify_wsl_directory(same, "Ubuntu-24.04"),
        WslDirectory::Resolved(path) if path == "/home/one/project"
    ));
    let verbatim = Path::new(r"\\?\UNC\wsl$\Ubuntu-24.04\home\one\project");
    assert!(matches!(
        classify_wsl_directory(verbatim, "ubuntu-24.04"),
        WslDirectory::Resolved(path) if path == "/home/one/project"
    ));
    assert!(matches!(
        classify_wsl_directory(same, "Debian"),
        WslDirectory::DifferentDistro
    ));
    assert!(matches!(
        classify_wsl_directory(Path::new(r"C:\work\project"), "Ubuntu"),
        WslDirectory::NeedsWslPath
    ));
}

#[test]
fn wsl_plans_use_argv_and_a_static_inner_script() {
    let mut spec = TerminalLaunchSpec::wsl(
        std::env::temp_dir().join("project"),
        "Ubuntu-24.04",
        CommandSpec::new(AllowedProgram::OpenCode, Vec::new()),
    );
    spec.command.args = vec!["-s".to_string(), "session with spaces".to_string()];
    assert!(spec.validate().is_ok());

    let probe =
        wsl_path_probe_spec("Ubuntu-24.04", Path::new(r"C:\work\project")).expect("wslpath plan");
    assert_eq!(probe.program, AllowedProgram::Wsl);
    assert_eq!(probe.args[3], "wslpath");
    assert_eq!(probe.sensitive_arg_indices, vec![6]);

    let plan =
        windows_wsl_plan(&spec, "/mnt/c/work/project".to_string()).expect("WSL terminal plan");
    assert_eq!(plan.command.program, AllowedProgram::Wsl);
    assert_eq!(plan.command.args[1], "Ubuntu-24.04");
    assert_eq!(plan.command.args[3], "/mnt/c/work/project");
    assert_eq!(plan.command.args[6], "AI_MANAGER_TOOL=opencode");
    assert_eq!(plan.command.args[9], WSL_TERMINAL_SCRIPT);
    assert_eq!(plan.command.args[10], "ai-manager-session");
    assert_eq!(&plan.command.args[11..], ["-s", "session with spaces"]);
    assert!(!plan.command.args[9].contains("opencode"));
    assert_eq!(plan.command.sensitive_arg_indices, vec![3, 11, 12]);
    assert!(plan.command.validate().is_ok());
}

#[test]
fn invalid_wsl_names_and_linux_working_directories_are_refused() {
    for distro in ["", "Ubuntu;rm", "name with space", &"x".repeat(65)] {
        let spec = TerminalLaunchSpec::wsl(
            std::env::temp_dir(),
            distro,
            CommandSpec::new(AllowedProgram::Codex, Vec::new()),
        );
        assert!(spec.validate().is_err(), "{distro:?}");
    }

    let valid = TerminalLaunchSpec::wsl(
        std::env::temp_dir(),
        "Ubuntu",
        CommandSpec::new(AllowedProgram::Codex, Vec::new()),
    );
    assert!(windows_wsl_plan(&valid, "relative/path".to_string()).is_err());
}

#[test]
fn windows_verbatim_paths_are_simplified_before_wslpath() {
    assert_eq!(
        windows_display_path(Path::new(r"\\?\C:\work\project")),
        r"C:\work\project"
    );
    assert_eq!(
        windows_display_path(Path::new(r"\\?\UNC\server\share\project")),
        r"\\server\share\project"
    );
}

#[test]
fn linux_plan_keeps_paths_and_session_arguments_as_argv() {
    use super::linux::{terminal_plan, LinuxTerminal};

    let mut spec = native_spec(AllowedProgram::Codex, "codex");
    spec.working_directory = std::env::temp_dir().join("project; touch never");
    spec.command.args = vec!["resume".to_string(), "session; still data".to_string()];
    let terminal_path = std::env::temp_dir().join("gnome-terminal");
    let env_path = std::env::temp_dir().join("env");
    let plan = terminal_plan(
        &spec,
        LinuxTerminal::GnomeTerminal,
        terminal_path.clone(),
        env_path.clone(),
    )
    .expect("Linux terminal plan");

    assert_eq!(plan.command.program, AllowedProgram::GnomeTerminal);
    assert_eq!(plan.command.program_path, Some(terminal_path.clone()));
    assert_eq!(plan.command.args[0], "--");
    assert_eq!(plan.command.args[1], env_path.to_string_lossy());
    assert_eq!(plan.command.args[2], "-C");
    assert_eq!(
        plan.command.args[3],
        spec.working_directory.to_string_lossy()
    );
    assert_eq!(plan.command.args[4], "PATH=/opt/homebrew/bin:/usr/bin");
    assert_eq!(
        plan.command.args[5],
        spec.command.program_path.unwrap().to_string_lossy()
    );
    assert_eq!(&plan.command.args[6..], ["resume", "session; still data"]);
    assert!(!plan.command.args.iter().any(|argument| argument == "-c"));
    assert_eq!(
        plan.command.redacted_display(),
        format!("{} -- *** *** *** *** *** *** ***", terminal_path.display())
    );
    assert!(plan.command.validate().is_ok());
}

#[test]
fn linux_terminal_prefixes_are_fixed_and_wsl_specs_are_refused() {
    use super::linux::{terminal_plan, LinuxTerminal};

    let spec = native_spec(AllowedProgram::ClaudeCode, "claude");
    for (terminal, binary, prefix) in [
        (LinuxTerminal::XdgTerminalExec, "xdg-terminal-exec", None),
        (LinuxTerminal::Konsole, "konsole", Some("-e")),
        (
            LinuxTerminal::XTerminalEmulator,
            "x-terminal-emulator",
            Some("-e"),
        ),
    ] {
        let env_path = std::env::temp_dir().join("env");
        let plan = terminal_plan(
            &spec,
            terminal,
            std::env::temp_dir().join(binary),
            env_path.clone(),
        )
        .expect("fixed terminal plan");
        match prefix {
            Some(prefix) => assert_eq!(plan.command.args[0], prefix),
            None => assert_eq!(plan.command.args[0], env_path.to_string_lossy()),
        }
    }

    let wsl = TerminalLaunchSpec::wsl(
        std::env::temp_dir(),
        "Ubuntu",
        CommandSpec::new(AllowedProgram::Codex, Vec::new()),
    );
    assert!(terminal_plan(
        &wsl,
        LinuxTerminal::Konsole,
        std::env::temp_dir().join("konsole"),
        std::env::temp_dir().join("env"),
    )
    .is_err());
}
