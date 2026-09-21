//! Per-app switch lock
//!
//! Ensures only one provider-switch operation runs at a time for a given app,
//! preventing concurrent switches from leaving `is_current` and the Live backup inconsistent.

use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::{Mutex, OwnedMutexGuard, RwLock};

/// One mutex per app type, so switch operations for the same app run serially.
///
/// Different apps (e.g. Claude and Codex) can switch in parallel.
#[derive(Clone, Default)]
pub struct SwitchLockManager {
    locks: Arc<RwLock<HashMap<String, Arc<Mutex<()>>>>>,
}

impl SwitchLockManager {
    pub fn new() -> Self {
        Self::default()
    }

    /// Acquires the switch lock for the given app.
    ///
    /// Returns an `OwnedMutexGuard`; while it is held, other switches for the same `app_type` queue up.
    pub async fn lock_for_app(&self, app_type: &str) -> OwnedMutexGuard<()> {
        let lock = {
            let locks = self.locks.read().await;
            if let Some(lock) = locks.get(app_type) {
                lock.clone()
            } else {
                drop(locks);
                let mut locks = self.locks.write().await;
                locks
                    .entry(app_type.to_string())
                    .or_insert_with(|| Arc::new(Mutex::new(())))
                    .clone()
            }
        };
        lock.lock_owned().await
    }

    /// Whether a switch / takeover operation is currently in progress for this app.
    ///
    /// This identifies the "takeover activation window": `set_takeover_for_app` holds this lock for
    /// its whole duration, and it writes the restore backup before it commits
    /// `proxy_config.enabled` and stamps the placeholder onto live. Inside that window, looking only
    /// at the flag and the placeholder misreads it as "not taken over", overwriting the live config
    /// that is mid-takeover.
    ///
    /// The key point is that this signal is **per app**. A global "the proxy process is running"
    /// flag cannot do this: while app A has takeover on, one leftover backup line for app B would
    /// count as B being taken over too, so saving B would again write only the backup, not the file.
    pub async fn is_locked_for_app(&self, app_type: &str) -> bool {
        let locks = self.locks.read().await;
        match locks.get(app_type) {
            Some(lock) => lock.try_lock().is_err(),
            None => false,
        }
    }
}
