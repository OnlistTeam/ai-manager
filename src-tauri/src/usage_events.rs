//! Live-refresh event module for usage stats.
//!
//! Whenever the `proxy_request_logs` table gets new rows (proxy logs, session
//! sync, archiving, etc.), this module emits a `usage-log-recorded` event to
//! the frontend so UsageDashboard can invalidate its query cache immediately
//! instead of waiting for the next poll cycle.
//!
//! Design notes:
//! - Global singleton AppHandle: the log-writing path doesn't hold an
//!   AppHandle, so it's shared via OnceCell.
//! - 200ms debounce merge: streaming responses and similar scenarios can
//!   write multiple log rows in a short window; merging them into one event
//!   avoids back-to-back frontend invalidations.
//! - Non-blocking writes: a failed notification is only logged as a warning,
//!   never propagated as an error.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::OnceLock;
use std::time::Duration;

use tauri::{AppHandle, Emitter};

/// Event name the frontend listens for.
pub const EVENT_USAGE_LOG_RECORDED: &str = "usage-log-recorded";

/// Debounce window: merges multiple notifications within 200ms.
const DEBOUNCE_WINDOW: Duration = Duration::from_millis(200);

static APP_HANDLE: OnceLock<AppHandle> = OnceLock::new();

/// Debounce flag: true means a scheduled emit is already pending and later
/// notifications get merged into it.
static EMIT_SCHEDULED: AtomicBool = AtomicBool::new(false);

/// Call once during app setup to inject the AppHandle.
///
/// Calling it again is harmless (OnceLock only accepts the first write), but
/// in practice it should only be called once, from `lib.rs::run`.
pub fn init(handle: AppHandle) {
    if APP_HANDLE.set(handle).is_err() {
        log::debug!("usage_events::init called again, ignoring");
    } else {
        log::info!("[usage-event] AppHandle injected, event push enabled");
    }
}

/// Notifies the frontend that a new usage log was recorded.
///
/// Callers do **not** need to hold an AppHandle and may call this from any
/// thread or write path. Debounced internally over 200ms; never blocks the
/// calling thread.
pub fn notify_log_recorded() {
    #[cfg(test)]
    TEST_NOTIFY_COUNT.with(|count| count.set(count.get().saturating_add(1)));

    // No AppHandle injected yet (typical in unit tests or before setup): bail out.
    let Some(handle) = APP_HANDLE.get() else {
        return;
    };

    // A scheduled emit already exists: merge this notification into it instead of spawning another thread.
    if EMIT_SCHEDULED.swap(true, Ordering::AcqRel) {
        return;
    }

    let handle = handle.clone();
    std::thread::spawn(move || {
        std::thread::sleep(DEBOUNCE_WINDOW);
        // Clear the flag before emitting: if a new notification arrives
        // during the emit, the next debounce window will reschedule it
        // rather than dropping it.
        EMIT_SCHEDULED.store(false, Ordering::Release);

        if let Err(e) = handle.emit(EVENT_USAGE_LOG_RECORDED, ()) {
            log::warn!("failed to emit {EVENT_USAGE_LOG_RECORDED}: {e}");
        }
    });
}

#[cfg(test)]
thread_local! {
    static TEST_NOTIFY_COUNT: std::cell::Cell<u32> = const { std::cell::Cell::new(0) };
}

#[cfg(test)]
pub(crate) fn take_test_notify_count() -> u32 {
    TEST_NOTIFY_COUNT.with(|count| count.replace(0))
}
