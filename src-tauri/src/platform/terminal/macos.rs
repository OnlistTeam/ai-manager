//! Which terminal takes over a session on macOS.
//!
//! The built-in Terminal and iTerm2 can only open a new window through AppleScript: the script body
//! is a compile-time constant, while the project path, tool path and session parameters each travel
//! through argv and are spliced into the command by `quoted form`. Every other terminal accepts
//! "an executable + argv", so they go through `open -na <App> --args`, with `/usr/bin/env -C`
//! providing the working directory and PATH — no temporary script is generated and no shell is
//! involved (consistent with the argv boundary ADR-0024 set for Linux).

use std::path::{Path, PathBuf};

use crate::domain::{AppError, TerminalAppId};
use crate::platform::command::{AllowedProgram, CommandSpec};
use crate::platform::TerminalLaunchSpec;

use super::{invalid_spec, TERMINAL_BRIDGE_TIMEOUT};

/// The tool name is injected through argv and the script only reads `argv`; this mirrors the Terminal
/// version word for word, differing only in who opens the window at the end.
pub(super) const ITERM_TERMINAL_SCRIPT: &str = r#"on run argv
    set project_path to item 1 of argv
    set tool_path to item 2 of argv
    set path_value to item 3 of argv
    set launch_command to "cd " & quoted form of project_path
    if path_value is not "" then
        set launch_command to launch_command & " && export PATH=" & quoted form of path_value
    end if
    set launch_command to launch_command & " && exec " & quoted form of tool_path
    if (count of argv) > 3 then
        repeat with argument_index from 4 to count of argv
            set launch_command to launch_command & " " & quoted form of (item argument_index of argv)
        end repeat
    end if
    tell application "iTerm"
        activate
        set target_window to (create window with default profile)
        tell current session of target_window
            write text launch_command
        end tell
    end tell
end run"#;

/// Terminals the user installed themselves live in the first two directories; Terminal.app only exists in the system directory.
const SEARCH_ROOTS: [&str; 3] = [
    "/Applications",
    "/Applications/Utilities",
    "/System/Applications/Utilities",
];

fn bundle_name(id: TerminalAppId) -> &'static str {
    match id {
        TerminalAppId::System => "Terminal",
        TerminalAppId::ITerm2 => "iTerm",
        TerminalAppId::Ghostty => "Ghostty",
        TerminalAppId::Kitty => "kitty",
        TerminalAppId::WezTerm => "WezTerm",
        TerminalAppId::Alacritty => "Alacritty",
    }
}

/// What follows `-e` is the argv to execute, not a shell line. kitty treats the first positional
/// argument as the program directly, while WezTerm needs the `start` subcommand first.
fn launch_flags(id: TerminalAppId) -> &'static [&'static str] {
    match id {
        TerminalAppId::Ghostty | TerminalAppId::Alacritty => &["-e"],
        TerminalAppId::Kitty => &[],
        TerminalAppId::WezTerm => &["start", "--"],
        // These two go through AppleScript and never reach this point.
        TerminalAppId::System | TerminalAppId::ITerm2 => &[],
    }
}

pub(super) fn app_path(id: TerminalAppId) -> Option<PathBuf> {
    let bundle = format!("{}.app", bundle_name(id));
    SEARCH_ROOTS
        .iter()
        .map(PathBuf::from)
        .chain(std::env::var_os("HOME").map(|home| PathBuf::from(home).join("Applications")))
        .map(|root| root.join(&bundle))
        .find(|candidate| candidate.is_dir())
}

/// The terminals actually installed on this machine, in the product's fixed order. Missing ones do not appear in the picker.
pub(super) fn installed() -> Vec<TerminalAppId> {
    TerminalAppId::ALL
        .into_iter()
        .filter(|id| app_path(*id).is_some())
        .collect()
}

pub(super) fn open_bridge_spec(
    spec: &TerminalLaunchSpec,
    id: TerminalAppId,
    app: &Path,
) -> Result<CommandSpec, AppError> {
    let tool_path = spec
        .command
        .program_path
        .as_ref()
        .ok_or_else(|| invalid_spec("error.commandSpec.terminalTargetNotAnchored"))?;

    let mut args = vec![
        "-na".to_string(),
        app.to_string_lossy().into_owned(),
        "--args".to_string(),
    ];
    args.extend(launch_flags(id).iter().map(|flag| (*flag).to_string()));
    args.push("/usr/bin/env".to_string());
    args.push("-C".to_string());
    args.push(spec.working_directory.to_string_lossy().into_owned());
    if let Some((_, path)) = spec.command.env.iter().find(|(key, _)| key == "PATH") {
        args.push(format!("PATH={path}"));
    }
    args.push(tool_path.to_string_lossy().into_owned());
    args.extend(spec.command.args.iter().cloned());

    // Everything after `--args` is local paths and session parameters, all hidden in the execution log.
    let sensitive = (3..args.len()).collect();
    Ok(CommandSpec::new(AllowedProgram::Open, args)
        .with_program_path(PathBuf::from("/usr/bin/open"))
        .with_sensitive_args(sensitive)
        .with_timeout(TERMINAL_BRIDGE_TIMEOUT))
}
