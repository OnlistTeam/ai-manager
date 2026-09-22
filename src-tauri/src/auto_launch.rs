//! Launch at login, without asking for a permission the feature does not need.
//!
//! On macOS the `auto-launch` crate defaults to driving Finder's login items
//! through AppleScript, and every call — including the read that renders the
//! settings toggle — runs `tell application "System Events" to …`. That makes
//! macOS show "AI Manager wants to control System Events" on first launch, for
//! a switch the user has not touched yet: an automation grant requested before
//! there is anything to automate.
//!
//! A launch agent needs no grant at all. Enabling writes one plist under
//! `~/Library/LaunchAgents`, disabling removes it, and reading the state is a
//! file existence check, so the settings page renders without a prompt.
//!
//! One consequence worth naming: a login item created by an earlier build's
//! AppleScript path is not removed here, because removing it would require the
//! very permission this change exists to avoid. That combination only affects
//! someone who turned the switch on before this change, and the single-instance
//! guard makes the duplicate launch harmless (ADR-0043).

use crate::error::AppError;
use auto_launch::{AutoLaunch, AutoLaunchBuilder};

/// The launch agent's `Label`, and therefore its file name. The bundle
/// identifier is what `launchctl` expects and keeps the file recognisable in
/// `~/Library/LaunchAgents` next to everything else.
#[cfg(target_os = "macos")]
const LAUNCH_AGENT_LABEL: &str = "tools.aimanager.desktop";

/// Initializes the AutoLaunch instance.
fn get_auto_launch() -> Result<AutoLaunch, AppError> {
    let exe_path = std::env::current_exe()
        .map_err(|e| AppError::Message(format!("Failed to get app path: {e}")))?;

    let mut builder = AutoLaunchBuilder::new();
    // `launchd` execs the path it is given, so it needs the real executable
    // inside the bundle rather than the `.app` directory. The bundle's
    // `Info.plist` still applies, so the window and Dock tile behave exactly as
    // they do when the bundle is opened from Finder.
    builder.set_app_path(&exe_path.to_string_lossy());

    #[cfg(target_os = "macos")]
    builder
        .set_app_name(LAUNCH_AGENT_LABEL)
        .set_use_launch_agent(true);

    // Windows uses the registry and Linux an XDG autostart entry; neither asks
    // for a permission, so both keep the display name.
    #[cfg(not(target_os = "macos"))]
    builder.set_app_name("AI Manager");

    builder
        .build()
        .map_err(|e| AppError::Message(format!("Failed to create AutoLaunch: {e}")))
}

/// Enables launch at login.
pub fn enable_auto_launch() -> Result<(), AppError> {
    let auto_launch = get_auto_launch()?;
    auto_launch
        .enable()
        .map_err(|e| AppError::Message(format!("Failed to enable launch at login: {e}")))?;
    log::info!("Launch at login enabled");
    Ok(())
}

/// Disables launch at login.
pub fn disable_auto_launch() -> Result<(), AppError> {
    let auto_launch = get_auto_launch()?;
    auto_launch
        .disable()
        .map_err(|e| AppError::Message(format!("Failed to disable launch at login: {e}")))?;
    log::info!("Launch at login disabled");
    Ok(())
}

/// Checks whether launch at login is enabled.
pub fn is_auto_launch_enabled() -> Result<bool, AppError> {
    let auto_launch = get_auto_launch()?;
    auto_launch
        .is_enabled()
        .map_err(|e| AppError::Message(format!("Failed to check launch-at-login status: {e}")))
}

// Every assertion here is about the macOS launch-agent path, so the module
// itself is macOS-only: on Windows and Linux an empty module would leave
// `use super::*` unused, which `-D warnings` turns into a build failure.
#[cfg(all(test, target_os = "macos"))]
mod tests {
    use super::*;

    /// The regression this module exists for: no code path may reach
    /// `osascript`, because reading the toggle would then prompt for an
    /// automation grant on first launch.
    #[test]
    fn macos_uses_a_launch_agent_rather_than_an_apple_script_login_item() {
        let auto_launch = get_auto_launch().expect("build the integration");
        assert_eq!(auto_launch.get_app_name(), LAUNCH_AGENT_LABEL);
        // `is_enabled` is a file existence check under a launch agent, so it
        // answers without a subprocess, without a prompt, and without failing
        // when the grant was refused.
        assert!(auto_launch.is_enabled().is_ok());
    }

    /// `launchd` cannot exec a directory, so the path must stay the real
    /// binary inside the bundle rather than being rewritten to the `.app`.
    #[test]
    fn the_launch_agent_points_at_the_executable_not_the_bundle() {
        let auto_launch = get_auto_launch().expect("build the integration");
        assert!(!auto_launch.get_app_path().ends_with(".app"));
        assert_eq!(
            auto_launch.get_app_path(),
            std::env::current_exe()
                .expect("the running executable")
                .to_string_lossy()
        );
    }
}
