//! Codex OAuth managed state
//!
//! Holds the Tauri State wrapper for OpenAI ChatGPT Plus/Pro OAuth (the reverse
//! proxy forwarder looks it up by type), plus the credential cleanup helper
//! used by `services::provider`.

use crate::proxy::providers::codex_oauth_auth::CodexOAuthManager;
use std::sync::Arc;

/// Codex OAuth auth state
///
/// `CodexOAuthManager` already uses fine-grained internal locks and all its
/// methods take `&self`, so we hold the `Arc` directly instead of wrapping it
/// in another `RwLock` — this avoids any caller holding a coarse lock across a
/// network refresh and blocking other paths (switching / auth center ops /
/// token reads).
pub struct CodexOAuthState(pub Arc<CodexOAuthManager>);
