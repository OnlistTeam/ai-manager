use std::sync::Mutex;

use super::{load_with, save_with, DesktopPreferencesBackend};
use crate::domain::DesktopPreferences;
use crate::settings::AppSettings;

struct FakeBackend {
    settings: Mutex<AppSettings>,
    auto_launch: Mutex<bool>,
    fail_next_settings_write: Mutex<bool>,
}

impl FakeBackend {
    fn new() -> Self {
        Self {
            settings: Mutex::new(AppSettings::default()),
            auto_launch: Mutex::new(false),
            fail_next_settings_write: Mutex::new(false),
        }
    }
}

impl DesktopPreferencesBackend for FakeBackend {
    fn settings(&self) -> AppSettings {
        self.settings.lock().expect("settings").clone()
    }

    fn write_settings(&self, settings: AppSettings) -> Result<(), String> {
        let mut fail = self.fail_next_settings_write.lock().expect("failure flag");
        if *fail {
            *fail = false;
            return Err("private /Users/alice/settings.json".to_string());
        }
        *self.settings.lock().expect("settings") = settings;
        Ok(())
    }

    fn auto_launch_enabled(&self) -> Result<bool, String> {
        Ok(*self.auto_launch.lock().expect("auto launch"))
    }

    fn set_auto_launch(&self, enabled: bool) -> Result<(), String> {
        *self.auto_launch.lock().expect("auto launch") = enabled;
        Ok(())
    }
}

fn enabled() -> DesktopPreferences {
    DesktopPreferences {
        launch_on_startup: true,
        silent_startup: true,
        show_in_tray: true,
        minimize_to_tray_on_close: true,
    }
}

#[test]
fn round_trip_uses_the_os_state_as_the_launch_authority() {
    let backend = FakeBackend::new();
    assert_eq!(save_with(&backend, enabled()).expect("save"), enabled());
    assert_eq!(load_with(&backend).expect("load"), enabled());
    assert!(backend.settings().launch_on_startup);
}

#[test]
fn unsafe_hidden_combinations_are_rejected_before_mutation() {
    let backend = FakeBackend::new();
    let error = save_with(
        &backend,
        DesktopPreferences {
            launch_on_startup: true,
            silent_startup: true,
            show_in_tray: false,
            minimize_to_tray_on_close: false,
        },
    )
    .expect_err("unsafe combination");
    assert_eq!(error.message_key, "error.desktopPreferences.invalid");
    assert!(!backend.auto_launch_enabled().expect("auto launch"));
}

#[test]
fn a_settings_write_failure_restores_the_auto_launch_state() {
    let backend = FakeBackend::new();
    *backend
        .fail_next_settings_write
        .lock()
        .expect("failure flag") = true;

    let error = save_with(&backend, enabled()).expect_err("write failure");
    assert_eq!(error.message_key, "error.desktopPreferences.saveFailed");
    assert!(!backend.auto_launch_enabled().expect("auto launch"));
    let restored = backend.settings();
    assert!(!restored.launch_on_startup);
    assert!(!restored.silent_startup);
    assert!(restored.show_in_tray);
    assert!(restored.minimize_to_tray_on_close);
}
