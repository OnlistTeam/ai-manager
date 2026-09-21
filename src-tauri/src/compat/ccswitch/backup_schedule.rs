//! Product facade over the inherited auto-backup settings.
//!
//! Upstream keeps `backup_interval_hours` (0 = off, default 24) and
//! `backup_retain_count` (default 10, minimum 1) in the settings file and
//! runs the periodic backup at startup. This is the only place that knows
//! those fields; the product sees a daily on/off switch and a retain count.

use std::sync::{Mutex, PoisonError};

use crate::domain::{AppError, BackupSchedule, ErrorCode};
use crate::platform::redact::{redact_secrets, truncate_tail};
use crate::settings::AppSettings;

/// Upstream `effective_backup_interval_hours` / `effective_backup_retain_count`
/// fall back to these when the file has no value.
const UPSTREAM_DEFAULT_INTERVAL_HOURS: u32 = 24;
const UPSTREAM_DEFAULT_RETAIN_COUNT: u32 = 10;

static WRITE_LOCK: Mutex<()> = Mutex::new(());

trait BackupScheduleBackend {
    fn settings(&self) -> AppSettings;
    fn write_settings(&self, settings: AppSettings) -> Result<(), String>;
}

struct PlatformBackend;

impl BackupScheduleBackend for PlatformBackend {
    fn settings(&self) -> AppSettings {
        crate::settings::get_settings()
    }

    fn write_settings(&self, settings: AppSettings) -> Result<(), String> {
        crate::settings::update_settings(settings).map_err(|error| error.to_string())
    }
}

fn save_failed(error: impl std::fmt::Display) -> AppError {
    AppError::new(
        ErrorCode::ConfigWriteFailed,
        "error.backup.scheduleSaveFailed",
    )
    .with_technical(truncate_tail(
        &redact_secrets(&error.to_string()),
        12,
        1_000,
    ))
    .with_remediation("error.remediation.retryOrViewDetails")
}

fn project(settings: &AppSettings) -> BackupSchedule {
    BackupSchedule::from_upstream(
        settings
            .backup_interval_hours
            .unwrap_or(UPSTREAM_DEFAULT_INTERVAL_HOURS),
        settings
            .backup_retain_count
            .unwrap_or(UPSTREAM_DEFAULT_RETAIN_COUNT),
    )
}

fn load_with(backend: &impl BackupScheduleBackend) -> BackupSchedule {
    project(&backend.settings())
}

fn save_with(
    backend: &impl BackupScheduleBackend,
    wanted: BackupSchedule,
) -> Result<BackupSchedule, AppError> {
    let wanted = wanted.normalized();
    // The guarded value is `()`: recover from a poisoned lock instead of
    // refusing every later save.
    let _guard = WRITE_LOCK.lock().unwrap_or_else(PoisonError::into_inner);
    let mut next = backend.settings();
    next.backup_interval_hours = Some(wanted.interval_hours());
    next.backup_retain_count = Some(wanted.retain_count);
    backend.write_settings(next).map_err(save_failed)?;
    Ok(load_with(backend))
}

pub struct BackupScheduleStore;

impl BackupScheduleStore {
    pub fn load() -> BackupSchedule {
        load_with(&PlatformBackend)
    }

    pub fn save(wanted: BackupSchedule) -> Result<BackupSchedule, AppError> {
        save_with(&PlatformBackend, wanted)
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Mutex;

    use super::{load_with, save_with, BackupScheduleBackend};
    use crate::domain::BackupSchedule;
    use crate::settings::AppSettings;

    struct FakeBackend {
        settings: Mutex<AppSettings>,
        fail_write: bool,
    }

    impl FakeBackend {
        fn with(interval: Option<u32>, retain: Option<u32>) -> Self {
            Self {
                settings: Mutex::new(AppSettings {
                    backup_interval_hours: interval,
                    backup_retain_count: retain,
                    ..AppSettings::default()
                }),
                fail_write: false,
            }
        }
    }

    impl BackupScheduleBackend for FakeBackend {
        fn settings(&self) -> AppSettings {
            self.settings.lock().expect("settings").clone()
        }

        fn write_settings(&self, settings: AppSettings) -> Result<(), String> {
            if self.fail_write {
                return Err("private /Users/alice/settings.json".to_string());
            }
            *self.settings.lock().expect("settings") = settings;
            Ok(())
        }
    }

    #[test]
    fn an_untouched_settings_file_reads_as_daily_backups_keeping_ten() {
        let backend = FakeBackend::with(None, None);
        assert_eq!(
            load_with(&backend),
            BackupSchedule {
                automatic: true,
                retain_count: 10,
            }
        );
    }

    #[test]
    fn switching_off_writes_a_zero_interval_and_on_restores_daily() {
        let backend = FakeBackend::with(Some(24), Some(10));
        let off = save_with(
            &backend,
            BackupSchedule {
                automatic: false,
                retain_count: 10,
            },
        )
        .expect("save off");
        assert!(!off.automatic);
        assert_eq!(backend.settings().backup_interval_hours, Some(0));

        let on = save_with(
            &backend,
            BackupSchedule {
                automatic: true,
                retain_count: 10,
            },
        )
        .expect("save on");
        assert!(on.automatic);
        assert_eq!(backend.settings().backup_interval_hours, Some(24));
    }

    #[test]
    fn a_zero_retain_count_is_raised_to_one_before_it_reaches_upstream() {
        let backend = FakeBackend::with(Some(24), Some(10));
        let saved = save_with(
            &backend,
            BackupSchedule {
                automatic: true,
                retain_count: 0,
            },
        )
        .expect("save");
        assert_eq!(saved.retain_count, 1);
        assert_eq!(backend.settings().backup_retain_count, Some(1));
    }

    #[test]
    fn a_failed_write_keeps_the_previous_schedule() {
        let mut backend = FakeBackend::with(Some(24), Some(10));
        backend.fail_write = true;
        let error = save_with(
            &backend,
            BackupSchedule {
                automatic: false,
                retain_count: 10,
            },
        )
        .expect_err("write failure");
        assert_eq!(error.message_key, "error.backup.scheduleSaveFailed");
        assert_eq!(
            error.remediation.as_deref(),
            Some("error.remediation.retryOrViewDetails")
        );
        assert!(load_with(&backend).automatic);
    }
}
