//! Product facade over the inherited device settings and auto-launch integration.

use std::sync::{Mutex, PoisonError};

use crate::domain::{AppError, DesktopPreferences, ErrorCode};
use crate::platform::redact::{redact_secrets, truncate_tail};
use crate::settings::AppSettings;

static WRITE_LOCK: Mutex<()> = Mutex::new(());

trait DesktopPreferencesBackend {
    fn settings(&self) -> AppSettings;
    fn write_settings(&self, settings: AppSettings) -> Result<(), String>;
    fn auto_launch_enabled(&self) -> Result<bool, String>;
    fn set_auto_launch(&self, enabled: bool) -> Result<(), String>;
}

struct PlatformBackend;

impl DesktopPreferencesBackend for PlatformBackend {
    fn settings(&self) -> AppSettings {
        crate::settings::get_settings()
    }

    fn write_settings(&self, settings: AppSettings) -> Result<(), String> {
        crate::settings::update_settings(settings).map_err(|error| error.to_string())
    }

    fn auto_launch_enabled(&self) -> Result<bool, String> {
        crate::auto_launch::is_auto_launch_enabled().map_err(|error| error.to_string())
    }

    fn set_auto_launch(&self, enabled: bool) -> Result<(), String> {
        if enabled {
            crate::auto_launch::enable_auto_launch()
        } else {
            crate::auto_launch::disable_auto_launch()
        }
        .map_err(|error| error.to_string())
    }
}

fn detail(error: impl std::fmt::Display) -> String {
    truncate_tail(&redact_secrets(&error.to_string()), 12, 1_000)
}

fn read_failed(error: impl std::fmt::Display) -> AppError {
    AppError::new(
        ErrorCode::UpstreamError,
        "error.desktopPreferences.readFailed",
    )
    .with_technical(detail(error))
    .with_remediation("error.remediation.retryOrViewDetails")
}

fn save_failed(error: impl std::fmt::Display) -> AppError {
    AppError::new(
        ErrorCode::ConfigWriteFailed,
        "error.desktopPreferences.saveFailed",
    )
    .with_technical(detail(error))
    .with_remediation("error.remediation.retryOrViewDetails")
}

fn project(settings: &AppSettings, launch_on_startup: bool) -> DesktopPreferences {
    DesktopPreferences {
        launch_on_startup,
        silent_startup: settings.silent_startup,
        show_in_tray: settings.show_in_tray,
        minimize_to_tray_on_close: settings.minimize_to_tray_on_close,
    }
}

fn load_with(backend: &impl DesktopPreferencesBackend) -> Result<DesktopPreferences, AppError> {
    let launch = backend.auto_launch_enabled().map_err(read_failed)?;
    Ok(project(&backend.settings(), launch))
}

fn rollback(
    backend: &impl DesktopPreferencesBackend,
    settings: AppSettings,
    auto_launch: bool,
) -> String {
    let settings = backend.write_settings(settings).err().map(detail);
    let auto_launch = backend.set_auto_launch(auto_launch).err().map(detail);
    format!(
        "rollback settings={}, autoLaunch={}",
        if settings.is_none() { "ok" } else { "failed" },
        if auto_launch.is_none() {
            "ok"
        } else {
            "failed"
        }
    )
}

fn save_with(
    backend: &impl DesktopPreferencesBackend,
    wanted: DesktopPreferences,
) -> Result<DesktopPreferences, AppError> {
    if !wanted.is_safe() {
        return Err(AppError::new(
            ErrorCode::ConfigParseFailed,
            "error.desktopPreferences.invalid",
        )
        .with_remediation("error.remediation.retryOrViewDetails"));
    }

    // The guarded value is `()`: recover from a poisoned lock instead of
    // refusing every later save.
    let _guard = WRITE_LOCK.lock().unwrap_or_else(PoisonError::into_inner);
    let previous_settings = backend.settings();
    let previous_auto_launch = backend.auto_launch_enabled().map_err(read_failed)?;

    if wanted.launch_on_startup != previous_auto_launch {
        backend
            .set_auto_launch(wanted.launch_on_startup)
            .map_err(|_| save_failed("auto-launch integration rejected the change"))?;
    }

    let mut next = previous_settings.clone();
    next.launch_on_startup = wanted.launch_on_startup;
    next.silent_startup = wanted.silent_startup;
    next.show_in_tray = wanted.show_in_tray;
    next.minimize_to_tray_on_close = wanted.minimize_to_tray_on_close;
    if let Err(error) = backend.write_settings(next) {
        let rollback = rollback(backend, previous_settings, previous_auto_launch);
        return Err(save_failed(format!(
            "settings write failed; {rollback}; {error}"
        )));
    }

    let verified = load_with(backend);
    if verified.as_ref().is_ok_and(|actual| *actual == wanted) {
        return verified;
    }

    let rollback = rollback(backend, previous_settings, previous_auto_launch);
    Err(save_failed(format!(
        "desktop preference verification failed; {rollback}"
    )))
}

pub struct DesktopPreferencesStore;

impl DesktopPreferencesStore {
    pub fn load() -> Result<DesktopPreferences, AppError> {
        load_with(&PlatformBackend)
    }

    pub fn save(wanted: DesktopPreferences) -> Result<DesktopPreferences, AppError> {
        save_with(&PlatformBackend, wanted)
    }
}

#[cfg(test)]
#[path = "desktop_preferences/tests.rs"]
mod tests;
