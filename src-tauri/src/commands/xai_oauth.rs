//! xAI OAuth managed state
//!
//! Holds the Tauri State wrapper: the reverse proxy forwarder looks up
//! `XaiOAuthManager` by type.

use crate::proxy::providers::xai_oauth_auth::XaiOAuthManager;
use std::sync::Arc;
use tokio::sync::RwLock;

pub struct XaiOAuthState(pub Arc<RwLock<XaiOAuthManager>>);
