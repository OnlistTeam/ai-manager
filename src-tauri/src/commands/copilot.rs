//! GitHub Copilot managed state
//!
//! Holds the Tauri State wrapper: the reverse proxy forwarder looks up
//! `CopilotAuthManager` by type.

use crate::proxy::providers::copilot_auth::CopilotAuthManager;
use std::sync::Arc;
use tokio::sync::RwLock;

/// Copilot auth state
pub struct CopilotAuthState(pub Arc<RwLock<CopilotAuthManager>>);
