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

use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

use serde::Serialize;
use tauri::Emitter;

use crate::application::routing_control::RoutingControl;
use crate::domain::RoutedTool;

pub const QUIT_REQUESTED_EVENT: &str = "app://quit-requested";
/// A quit requested again within this time after asking goes ahead.
const ASKED_RECENTLY_MS: u64 = 60_000;

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

    /// The user confirmed: quit, and do not ask again.
    pub fn confirm_and_quit(app_handle: &tauri::AppHandle) {
        CONFIRMED.store(true, Ordering::SeqCst);
        app_handle.exit(0);
    }
}

#[cfg(test)]
mod tests {
    use super::{should_ask, ASKED_RECENTLY_MS};
    use crate::domain::{RoutedTool, RoutingPickup, ToolId};

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
