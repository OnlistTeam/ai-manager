//! Product-safe device preferences for the native desktop shell.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct DesktopPreferences {
    pub launch_on_startup: bool,
    pub silent_startup: bool,
    pub show_in_tray: bool,
    pub minimize_to_tray_on_close: bool,
}

impl DesktopPreferences {
    pub fn is_safe(self) -> bool {
        (!self.silent_startup || (self.launch_on_startup && self.show_in_tray))
            && (!self.minimize_to_tray_on_close || self.show_in_tray)
    }
}

#[cfg(test)]
mod tests {
    use super::DesktopPreferences;

    #[test]
    fn hidden_behaviour_always_keeps_a_way_back_into_the_app() {
        let safe = DesktopPreferences {
            launch_on_startup: true,
            silent_startup: true,
            show_in_tray: true,
            minimize_to_tray_on_close: true,
        };
        assert!(safe.is_safe());

        assert!(!DesktopPreferences {
            show_in_tray: false,
            ..safe
        }
        .is_safe());
        assert!(!DesktopPreferences {
            launch_on_startup: false,
            ..safe
        }
        .is_safe());
    }
}
