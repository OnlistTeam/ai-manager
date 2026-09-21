use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum AppUpdatePhase {
    Unconfigured,
    Idle,
    Checking,
    Downloading,
    Ready,
    UpToDate,
    Failed,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AppUpdateStatus {
    pub current_version: String,
    pub available_version: Option<String>,
    pub channel_ready: bool,
    pub phase: AppUpdatePhase,
    pub downloaded_bytes: u64,
    pub total_bytes: Option<u64>,
    pub attempt: u8,
    pub max_attempts: u8,
}

impl AppUpdateStatus {
    pub fn new(current_version: String, channel_ready: bool, max_attempts: u8) -> Self {
        Self {
            current_version,
            available_version: None,
            channel_ready,
            phase: if channel_ready {
                AppUpdatePhase::Idle
            } else {
                AppUpdatePhase::Unconfigured
            },
            downloaded_bytes: 0,
            total_bytes: None,
            attempt: 0,
            max_attempts: if channel_ready { max_attempts } else { 0 },
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{AppUpdatePhase, AppUpdateStatus};

    #[test]
    fn a_missing_trust_channel_is_explicitly_unconfigured() {
        let status = AppUpdateStatus::new("1.2.3".to_string(), false, 3);
        assert_eq!(status.phase, AppUpdatePhase::Unconfigured);
        assert!(!status.channel_ready);
        assert_eq!(status.available_version, None);
        assert_eq!(status.attempt, 0);
        assert_eq!(status.max_attempts, 0);
    }
}
