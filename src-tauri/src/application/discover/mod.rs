//! The Discover section of the Skills and MCP pages (ADR-0063): what can be
//! added, whether it already is, and adding it.
//!
//! Sources: a featured list of MCP servers kept in the product, the official
//! MCP Registry for searches, and skills.sh for Skills. Adding goes through
//! the same install services the rest of the page uses, so capability gates,
//! the per-tool task lock, verification and the Skill mirror fallback apply
//! unchanged.

mod catalog;
mod fetch;
mod icons;
mod install;
mod matching;
mod registry;
mod skill_list;
mod skills_sh;

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::{Arc, Mutex, MutexGuard, OnceLock, PoisonError};
use std::time::{Duration, Instant};

use crate::application::extension_directory::supports;
use crate::compat::ccswitch::discover::{mcp_holdings, mcp_transports, skill_holdings, McpHolding};
use crate::compat::ccswitch::tools::capabilities_for;
use crate::domain::{
    AppError, DesktopAppId, DiscoverMcpList, DiscoverMcpReach, DiscoverSkillList, ErrorCode,
    ExtensionKind, ExtensionScope, SkillCatalogItem, ToolId,
};
use crate::platform::Platform;

use catalog::{featured, MarketServer};
use fetch::{default_cache_dir, FetchError, Fetcher, HttpFetcher};
use skills_sh::SkillsShEntry;

pub use install::{install_mcp, install_skill, open_link};

const REGISTRY_URL: &str = "https://registry.modelcontextprotocol.io/v0/servers";
const SKILLS_SH_URL: &str = "https://www.skills.sh";

const POPULAR_FOR: Duration = Duration::from_secs(6 * 60 * 60);
const POPULAR_RETRY_AFTER: Duration = Duration::from_secs(5 * 60);
const SEARCH_FOR: Duration = Duration::from_secs(10 * 60);
const MAX_QUERY_CHARS: usize = 200;
const MAX_SKILLS: usize = 60;
const PER_SOURCE_ON_FIRST_SCREEN: usize = 4;
const MAX_DESCRIBED: usize = 60;
const DESCRIBE_AT_ONCE: usize = 6;
const MAX_SEEN_SERVERS: usize = 2000;

/// What this session has learnt: kept in memory, the parts worth keeping
/// across launches also on disk.
#[derive(Default)]
pub(crate) struct DiscoverState {
    /// Registry servers shown this session, so one can be added by its id
    /// and its icon may be fetched.
    seen: Mutex<HashMap<String, MarketServer>>,
    registry_searches: Mutex<HashMap<String, (Instant, Vec<MarketServer>)>>,
    /// skills.sh's most installed, and when to read them again.
    popular: tokio::sync::Mutex<Option<(Instant, Vec<SkillsShEntry>)>>,
    skill_searches: Mutex<HashMap<String, (Instant, Vec<SkillsShEntry>)>>,
    /// Skill descriptions by id, read from disk once.
    descriptions: Mutex<Option<HashMap<String, String>>>,
    /// Where a skills.sh Skill was found in its repository.
    resolved: Mutex<HashMap<String, SkillCatalogItem>>,
}

fn lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(PoisonError::into_inner)
}

pub struct DiscoverService {
    fetcher: Arc<dyn Fetcher>,
    registry_url: String,
    skills_url: String,
    cache_dir: PathBuf,
    state: Arc<DiscoverState>,
}

impl DiscoverService {
    /// The live service: the product's outbound client, the real sources, the
    /// product cache directory and this process's session state.
    pub fn system() -> Self {
        static STATE: OnceLock<Arc<DiscoverState>> = OnceLock::new();
        Self::new(
            Arc::new(HttpFetcher),
            REGISTRY_URL.to_string(),
            SKILLS_SH_URL.to_string(),
            default_cache_dir(),
            STATE.get_or_init(Arc::default).clone(),
        )
    }

    pub(crate) fn new(
        fetcher: Arc<dyn Fetcher>,
        registry_url: String,
        skills_url: String,
        cache_dir: PathBuf,
        state: Arc<DiscoverState>,
    ) -> Self {
        Self {
            fetcher,
            registry_url,
            skills_url,
            cache_dir,
            state,
        }
    }

    // ---- MCP ---------------------------------------------------------------

    /// Featured servers matching the query, then the registry's.
    pub(crate) async fn servers(&self, query: &str) -> (Vec<MarketServer>, Option<AppError>) {
        let query = query.trim().to_lowercase();
        let mut out = featured()
            .iter()
            .filter(|server| {
                query.is_empty()
                    || [
                        server.title.as_str(),
                        server.id.as_str(),
                        server.publisher.as_deref().unwrap_or_default(),
                    ]
                    .iter()
                    .any(|field| field.to_lowercase().contains(&query))
            })
            .cloned()
            .collect::<Vec<_>>();
        if query.is_empty() {
            return (out, None);
        }
        match self.search_registry(&query).await {
            Ok(found) => {
                let keys = out
                    .iter()
                    .map(|server| matching::server_key(&server.spec))
                    .collect::<Vec<_>>();
                out.extend(
                    found
                        .into_iter()
                        .filter(|server| !keys.contains(&matching::server_key(&server.spec))),
                );
                (out, None)
            }
            Err(error) => (
                out,
                Some(
                    AppError::new(
                        ErrorCode::NetworkError,
                        "error.discover.registryUnreachable",
                    )
                    .with_technical(error.0)
                    .with_remediation("error.remediation.checkInternetConnection"),
                ),
            ),
        }
    }

    async fn search_registry(&self, query: &str) -> Result<Vec<MarketServer>, FetchError> {
        if let Some((at, found)) = lock(&self.state.registry_searches).get(query) {
            if at.elapsed() < SEARCH_FOR {
                return Ok(found.clone());
            }
        }
        let url = url::Url::parse_with_params(
            &self.registry_url,
            &[("search", query), ("limit", "100"), ("version", "latest")],
        )
        .map_err(|error| FetchError(error.to_string()))?;
        let body = self
            .fetcher
            .get(url.to_string(), "application/json", 8 << 20)
            .await?;
        let found = registry::parse_search(&body, query).map_err(FetchError)?;
        {
            let mut seen = lock(&self.state.seen);
            if seen.len() > MAX_SEEN_SERVERS {
                seen.clear();
            }
            for server in &found {
                seen.insert(server.id.clone(), server.clone());
            }
        }
        lock(&self.state.registry_searches)
            .insert(query.to_string(), (Instant::now(), found.clone()));
        Ok(found)
    }

    /// A featured server, or a registry server shown this session.
    pub(crate) fn server(&self, id: &str) -> Option<MarketServer> {
        catalog::featured_server(id)
            .cloned()
            .or_else(|| lock(&self.state.seen).get(id).cloned())
    }

    pub(crate) async fn mcp_list(
        &self,
        query: &str,
        holdings: &[McpHolding],
    ) -> Result<DiscoverMcpList, AppError> {
        validate_query(query)?;
        let (servers, source_error) = self.servers(query).await;
        Ok(DiscoverMcpList {
            items: servers
                .iter()
                .map(|server| server.view(matching::added_server(&server.spec, holdings)))
                .collect(),
            reach: mcp_reach(),
            source_error,
        })
    }

    // ---- Icons -------------------------------------------------------------

    /// A card's picture as a data URL, fetched once and then read from disk.
    pub async fn icon(&self, url: &str) -> Result<String, AppError> {
        let offered = {
            let seen = lock(&self.state.seen);
            let mut offered = featured()
                .iter()
                .filter_map(|server| server.icon.clone())
                .collect::<Vec<_>>();
            offered.extend(seen.values().filter_map(|server| server.icon.clone()));
            offered
        };
        if !icons::allowed(url, offered.iter().map(String::as_str)) {
            return Err(
                AppError::new(ErrorCode::PermissionDenied, "error.discover.iconRefused")
                    .with_technical("the address is not a picture the Discover section offered"),
            );
        }
        let path = icons::cache_path(&self.cache_dir, url);
        if let Some(cached) = icons::read_cached(&path) {
            return Ok(cached);
        }
        let unavailable = |detail: String| {
            AppError::new(ErrorCode::NetworkError, "error.discover.iconUnavailable")
                .with_technical(detail)
        };
        let bytes = self
            .fetcher
            .get(url.to_string(), "image/*", icons::MAX_ICON_BYTES)
            .await
            .map_err(|error| unavailable(error.0))?;
        let mime = icons::sniff(&bytes).ok_or_else(|| unavailable("not a picture".to_string()))?;
        let data = icons::data_url(mime, &bytes);
        fetch::write_file(&path, data.as_bytes());
        Ok(data)
    }

    // ---- Skill resolution --------------------------------------------------

    pub(crate) fn resolved(&self, id: &str) -> Option<SkillCatalogItem> {
        lock(&self.state.resolved).get(id).cloned()
    }

    pub(crate) fn remember_resolved(&self, id: &str, item: SkillCatalogItem) {
        lock(&self.state.resolved).insert(id.to_string(), item);
    }
}

fn validate_query(query: &str) -> Result<(), AppError> {
    if query.chars().count() > MAX_QUERY_CHARS || query.chars().any(char::is_control) {
        return Err(
            AppError::new(ErrorCode::Internal, "error.discover.queryInvalid")
                .with_technical("the search words are too long or not one line"),
        );
    }
    Ok(())
}

/// Every app on this machine's platform that can take MCP servers, with the
/// transports its file can express.
pub(crate) fn mcp_scopes() -> Vec<ExtensionScope> {
    let mut scopes = ToolId::ALL
        .iter()
        .copied()
        .filter(|tool| supports(ExtensionKind::Mcp, &capabilities_for(*tool)))
        .map(ExtensionScope::tool)
        .collect::<Vec<_>>();
    if matches!(Platform::current(), Platform::MacOs | Platform::Windows) {
        scopes.push(ExtensionScope::desktop_app(DesktopAppId::ClaudeDesktop));
    }
    scopes
}

pub(crate) fn mcp_reach() -> Vec<DiscoverMcpReach> {
    mcp_scopes()
        .into_iter()
        .map(|scope| DiscoverMcpReach {
            scope,
            transports: mcp_transports(scope),
        })
        .collect()
}

/// The Discover list for MCP, with what the user already has marked.
pub async fn list_mcp(
    app_handle: tauri::AppHandle,
    query: String,
) -> Result<DiscoverMcpList, AppError> {
    let holdings = blocking(move || mcp_holdings(&app_handle)).await?;
    DiscoverService::system().mcp_list(&query, &holdings).await
}

/// The Discover list for Skills, with what the user already has marked.
pub async fn list_skills(
    app_handle: tauri::AppHandle,
    query: String,
) -> Result<DiscoverSkillList, AppError> {
    let holdings = blocking(move || skill_holdings(&app_handle)).await?;
    DiscoverService::system()
        .skill_list(&query, &holdings)
        .await
}

pub(crate) async fn blocking<T, F>(work: F) -> Result<T, AppError>
where
    T: Send + 'static,
    F: FnOnce() -> Result<T, AppError> + Send + 'static,
{
    tauri::async_runtime::spawn_blocking(work)
        .await
        .map_err(|error| {
            AppError::new(ErrorCode::Internal, "error.command.joinFailed")
                .with_technical(error.to_string())
                .with_remediation("error.remediation.retryOrViewDetails")
        })?
}

#[cfg(test)]
mod tests;
