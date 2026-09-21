//! Automatic backup policy use case.
//!
//! As thin as `product_settings`: the command layer must not know that the
//! policy is stored as an upstream interval in hours.

use crate::compat::ccswitch::backup_schedule::BackupScheduleStore;
use crate::domain::{AppError, BackupSchedule};

pub struct BackupScheduleService;

impl BackupScheduleService {
    pub fn load() -> BackupSchedule {
        BackupScheduleStore::load()
    }

    pub fn save(schedule: BackupSchedule) -> Result<BackupSchedule, AppError> {
        BackupScheduleStore::save(schedule)
    }
}
