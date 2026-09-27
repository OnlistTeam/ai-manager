//! Holding a quit while tools are routed through AI Manager (ADR-0054).
//!
//! Quitting stops the local gateway and puts every routed tool back. A
//! session of such a tool that is already open may still hold the gateway's
//! address, so the first user-initiated quit while any tool is routed is
//! held: the window comes forward (the shell does that) and the renderer
//! asks, naming the tools.
//! Its confirmation quits for real. A second quit request shortly after
//! goes ahead without asking again, so a window that cannot answer never
//! keeps AI Manager from quitting.
//!
//! macOS also ends the app without a quit request: a logout, restart or
//! shutdown, the app menu's Quit and the Dock's Quit all go through
//! NSApplication `terminate:`, which Tauri reports only as the end of the
//! event loop, with no reason attached and no way to hold it. Nothing is
//! asked there; the routed tools are put back before the process ends.

use std::future::Future;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use serde::Serialize;
use tauri::Emitter;

use crate::application::routing_control::RoutingControl;
use crate::domain::RoutedTool;

pub const QUIT_REQUESTED_EVENT: &str = "app://quit-requested";
/// A quit requested again within this time after asking goes ahead.
const ASKED_RECENTLY_MS: u64 = 60_000;
/// How long putting the routed tools back may hold a process the system is
/// ending. The next launch puts back whatever did not finish.
const ENDING_CLEANUP_BUDGET: Duration = Duration::from_secs(5);

static CONFIRMED: AtomicBool = AtomicBool::new(false);
static ASKED_AT_MS: AtomicU64 = AtomicU64::new(0);

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct QuitRequested {
    tools: Vec<RoutedTool>,
}

fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|elapsed| u64::try_from(elapsed.as_millis()).unwrap_or(u64::MAX))
        .unwrap_or(0)
}

/// Whether a quit request should be held to ask first, given what is known.
fn should_ask(confirmed: bool, asked_at_ms: u64, now_ms: u64, routed: &[RoutedTool]) -> bool {
    let asked_recently = asked_at_ms != 0 && now_ms.saturating_sub(asked_at_ms) < ASKED_RECENTLY_MS;
    !confirmed && !asked_recently && !routed.is_empty()
}

/// Runs `work` for at most `budget`; `false` when it did not finish.
async fn within_budget(budget: Duration, work: impl Future<Output = ()>) -> bool {
    tokio::time::timeout(budget, work).await.is_ok()
}

pub struct QuitGuard;

impl QuitGuard {
    /// The routed tools to ask about before this quit, or `None` to quit
    /// now. Asking is remembered, so the next request goes ahead.
    pub async fn tools_to_ask(app_handle: &tauri::AppHandle) -> Option<Vec<RoutedTool>> {
        let now = now_ms();
        let tools = RoutingControl::routed_tools(app_handle)
            .await
            .unwrap_or_default();
        let ask = should_ask(
            CONFIRMED.load(Ordering::SeqCst),
            ASKED_AT_MS.load(Ordering::SeqCst),
            now,
            &tools,
        );
        if !ask {
            return None;
        }
        ASKED_AT_MS.store(now, Ordering::SeqCst);
        Some(tools)
    }

    /// Puts the question to the window, which the caller has brought
    /// forward. Returns `false` when it could not be asked.
    pub fn ask(app_handle: &tauri::AppHandle, tools: Vec<RoutedTool>) -> bool {
        match app_handle.emit(QUIT_REQUESTED_EVENT, QuitRequested { tools }) {
            Ok(()) => true,
            Err(error) => {
                log::warn!("Could not ask before quitting; quitting now: {error}");
                false
            }
        }
    }

    /// The event loop is ending (see the module note): put every routed tool
    /// back, within a bounded time. Runs on the main thread, which blocks
    /// until the clean-up is done; a quit that already cleaned up finds
    /// nothing left to do.
    pub fn clean_up_as_the_app_ends(app_handle: &tauri::AppHandle) {
        let handle = app_handle.clone();
        let finished =
            tauri::async_runtime::block_on(within_budget(ENDING_CLEANUP_BUDGET, async move {
                RoutingControl::release_before_exit(&handle).await
            }));
        if !finished {
            log::error!(
                "Putting the routed tools back did not finish in time; the next launch does it"
            );
        }
    }

    /// The user confirmed: quit, and do not ask again.
    pub fn confirm_and_quit(app_handle: &tauri::AppHandle) {
        CONFIRMED.store(true, Ordering::SeqCst);
        app_handle.exit(0);
    }
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use super::{should_ask, within_budget, ASKED_RECENTLY_MS};
    use crate::domain::{RoutedTool, RoutingPickup, ToolId};

    #[tokio::test]
    async fn the_clean_up_of_an_ending_process_is_bounded() {
        assert!(within_budget(Duration::from_millis(50), async {}).await);
        assert!(
            !within_budget(Duration::from_millis(20), std::future::pending::<()>()).await,
            "a stuck clean-up must not keep the system from ending the app"
        );
    }

    #[test]
    fn asks_once_when_tools_are_routed_and_never_blocks_a_second_quit() {
        let routed = [RoutedTool {
            tool: ToolId::ClaudeCode,
            pickup: RoutingPickup::Live,
        }];
        assert!(should_ask(false, 0, 1_000, &routed));
        assert!(!should_ask(false, 0, 1_000, &[]), "nothing routed");
        assert!(!should_ask(true, 0, 1_000, &routed), "already confirmed");
        assert!(
            !should_ask(false, 1_000, 1_000 + ASKED_RECENTLY_MS - 1, &routed),
            "asked moments ago"
        );
        assert!(should_ask(false, 1_000, 1_000 + ASKED_RECENTLY_MS, &routed));
    }
}
