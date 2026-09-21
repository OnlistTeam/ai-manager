//! The in-memory queue of links waiting for the user's confirmation.
//!
//! Nothing here writes. A link lives in this queue only until the user accepts
//! it, dismisses it, or it expires (ADR-0029 decision 5).

use std::collections::VecDeque;
use std::sync::Mutex;

use crate::domain::{AppError, DeepLinkIntent, DeepLinkPreview, ErrorCode, LinkOrigin};

/// Small on purpose. A window that queued more than this has almost certainly
/// been pointed at a link generator rather than at one vendor's install button.
const MAX_PENDING: usize = 8;
const TTL_SECONDS: i64 = 10 * 60;

#[derive(Debug)]
pub struct PendingDeepLink {
    pub id: String,
    pub origin: LinkOrigin,
    pub intent: DeepLinkIntent,
    pub expires_at: i64,
}

/// The queue is registered as Tauri state and shared by every path.
#[derive(Default)]
pub struct DeepLinkQueue {
    entries: Mutex<VecDeque<PendingDeepLink>>,
}

impl DeepLinkQueue {
    pub fn new() -> Self {
        Self::default()
    }

    /// Queues one parsed link and returns its opaque identity. The oldest entry
    /// is dropped when the queue is full: a fresh link the user just clicked is
    /// worth more than one they ignored.
    pub fn push(&self, origin: LinkOrigin, intent: DeepLinkIntent, now: i64) -> String {
        let id = uuid::Uuid::new_v4().to_string();
        let mut entries = self.lock();
        retain_live(&mut entries, now);
        while entries.len() >= MAX_PENDING {
            entries.pop_front();
        }
        entries.push_back(PendingDeepLink {
            id: id.clone(),
            origin,
            intent,
            expires_at: now + TTL_SECONDS,
        });
        id
    }

    pub fn len(&self, now: i64) -> usize {
        let mut entries = self.lock();
        retain_live(&mut entries, now);
        entries.len()
    }

    /// Runs `project` over every live entry, oldest first.
    pub fn project<F>(&self, now: i64, project: F) -> Vec<DeepLinkPreview>
    where
        F: Fn(&PendingDeepLink) -> DeepLinkPreview,
    {
        let mut entries = self.lock();
        retain_live(&mut entries, now);
        entries.iter().map(project).collect()
    }

    pub fn with<F, T>(&self, id: &str, now: i64, read: F) -> Result<T, AppError>
    where
        F: FnOnce(&PendingDeepLink) -> T,
    {
        let mut entries = self.lock();
        retain_live(&mut entries, now);
        entries
            .iter()
            .find(|entry| entry.id == id)
            .map(read)
            .ok_or_else(gone)
    }

    /// Removes and returns the entry. Confirming consumes it, so pressing the
    /// button twice cannot import twice.
    pub fn take(&self, id: &str, now: i64) -> Result<PendingDeepLink, AppError> {
        let mut entries = self.lock();
        retain_live(&mut entries, now);
        let index = entries
            .iter()
            .position(|entry| entry.id == id)
            .ok_or_else(gone)?;
        entries.remove(index).ok_or_else(gone)
    }

    /// Dismissal is the same removal, minus the caller's interest in the value.
    pub fn dismiss(&self, id: &str, now: i64) -> Result<(), AppError> {
        self.take(id, now).map(|_| ())
    }

    /// A poisoned queue cannot corrupt anything: it holds no persistent state,
    /// so recovering the data and carrying on is correct.
    fn lock(&self) -> std::sync::MutexGuard<'_, VecDeque<PendingDeepLink>> {
        self.entries
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
    }
}

fn retain_live(entries: &mut VecDeque<PendingDeepLink>, now: i64) {
    entries.retain(|entry| entry.expires_at > now);
}

/// Expired, already confirmed, dismissed, or never queued at all: the renderer
/// gets one answer for all four, because it can act on all four the same way.
fn gone() -> AppError {
    AppError::new(ErrorCode::OperationConflict, "error.deepLink.notPending")
        .with_technical("the requested link is no longer waiting for confirmation")
        .with_remediation("error.remediation.retryOrViewDetails")
}

pub fn now_seconds() -> i64 {
    chrono::Utc::now().timestamp()
}

#[cfg(test)]
mod tests {
    use super::{DeepLinkQueue, MAX_PENDING, TTL_SECONDS};
    use crate::domain::deep_link::{parse, LinkOrigin};

    fn intent(name: &str) -> crate::domain::DeepLinkIntent {
        parse(
            &format!(
                "aimanager://v1/import?resource=provider&app=claude&name={name}&endpoint=https://api.example.test"
            ),
            LinkOrigin::Argv,
        )
        .expect("a valid link")
    }

    #[test]
    fn a_full_queue_drops_the_oldest_entry_and_keeps_the_newest() {
        let queue = DeepLinkQueue::new();
        let first = queue.push(LinkOrigin::Argv, intent("First"), 0);
        for index in 1..MAX_PENDING {
            queue.push(LinkOrigin::Argv, intent(&format!("Link{index}")), 0);
        }
        assert_eq!(queue.len(0), MAX_PENDING);

        let newest = queue.push(LinkOrigin::Argv, intent("Newest"), 0);
        assert_eq!(queue.len(0), MAX_PENDING);
        assert!(queue.with(&first, 0, |_| ()).is_err(), "oldest is evicted");
        assert!(queue.with(&newest, 0, |_| ()).is_ok());
    }

    #[test]
    fn confirming_consumes_the_entry_so_a_second_press_cannot_import_twice() {
        let queue = DeepLinkQueue::new();
        let id = queue.push(LinkOrigin::Paste, intent("Once"), 0);

        queue.take(&id, 0).expect("the first take succeeds");
        let second = queue.take(&id, 0).expect_err("the second must not");
        assert_eq!(second.message_key, "error.deepLink.notPending");
    }

    #[test]
    fn an_expired_entry_is_neither_listed_nor_confirmable() {
        let queue = DeepLinkQueue::new();
        let id = queue.push(LinkOrigin::Argv, intent("Stale"), 0);

        assert_eq!(queue.len(TTL_SECONDS - 1), 1);
        assert_eq!(queue.len(TTL_SECONDS + 1), 0);
        assert_eq!(
            queue
                .take(&id, TTL_SECONDS + 1)
                .expect_err("expiry removes it")
                .message_key,
            "error.deepLink.notPending"
        );
    }

    #[test]
    fn dismissing_removes_the_entry_without_returning_anything() {
        let queue = DeepLinkQueue::new();
        let id = queue.push(LinkOrigin::Paste, intent("Rejected"), 0);

        queue.dismiss(&id, 0).expect("dismissal succeeds");
        assert_eq!(queue.len(0), 0);
        assert!(queue.dismiss(&id, 0).is_err());
    }
}
