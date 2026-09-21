use crate::database::Database;
use crate::proxy::providers::codex_oauth_auth::CodexOAuthManager;
use crate::services::{ProxyService, UsageCache};
use std::sync::{Arc, Mutex as StdMutex};
use tokio::sync::Mutex;

/// Global application state
#[derive(Clone)]
pub struct AppState {
    pub db: Arc<Database>,
    pub proxy_service: ProxyService,
    /// Product routing mutations are serialized across apps so a global
    /// restore cannot race a per-app takeover or failover queue change.
    pub routing_mutation_lock: Arc<Mutex<()>>,
    /// Serializes provider file/database mutations coming from the main UI,
    /// system tray and inherited compatibility entry points.
    pub provider_mutation_lock: Arc<StdMutex<()>>,
    pub usage_cache: Arc<UsageCache>,
    // Internally uses fine-grained locks (accounts/access_tokens/refresh_locks); all
    // methods take `&self`, so no outer RwLock is needed, avoiding chained blocking
    // from holding a coarse lock across a network refresh.
    pub codex_oauth_manager: Arc<CodexOAuthManager>,
}

impl AppState {
    /// Creates new application state.
    pub fn new(db: Arc<Database>) -> Self {
        let codex_oauth_manager = Arc::new(CodexOAuthManager::new(
            crate::infrastructure::paths::product_data_dir(),
        ));
        let proxy_service =
            ProxyService::new_with_codex_oauth_manager(db.clone(), codex_oauth_manager.clone());

        Self {
            db,
            proxy_service,
            routing_mutation_lock: Arc::new(Mutex::new(())),
            provider_mutation_lock: Arc::new(StdMutex::new(())),
            usage_cache: Arc::new(UsageCache::new()),
            codex_oauth_manager,
        }
    }
}
