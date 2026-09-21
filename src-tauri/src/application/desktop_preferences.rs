//! Native desktop preference use case.

use crate::compat::ccswitch::desktop_preferences::DesktopPreferencesStore;
use crate::domain::{AppError, DesktopPreferences};

pub struct DesktopPreferencesService;

impl DesktopPreferencesService {
    pub fn load() -> Result<DesktopPreferences, AppError> {
        DesktopPreferencesStore::load()
    }

    pub fn save(settings: DesktopPreferences) -> Result<DesktopPreferences, AppError> {
        DesktopPreferencesStore::save(settings)
    }
}
