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
mod skills_sh;

use std::collections::{BTreeMap, HashMap};
use std::path::PathBuf;
use std::sync::{Arc, Mutex, MutexGuard, OnceLock, PoisonError};
use std::time::{Duration, Instant};

use futures::stream::{self, StreamExt};

use crate::application::extension_directory::supports;
use crate::compat::ccswitch::discover::{
    mcp_holdings, mcp_transports, skill_holdings, McpHolding, SkillHolding,
};
use crate::compat::ccswitch::tools::capabilities_for;
use crate::domain::{
    AppError, DesktopAppId, DiscoverMcpList, DiscoverMcpReach, DiscoverSkill, DiscoverSkillList,
    ErrorCode, ExtensionKind, ExtensionScope, SkillCatalogItem, ToolId,
};
use crate::platform::Platform;

use catalog::{featured, owner_avatar, MarketServer};
use fetch::{default_cache_dir, read_json, write_json, FetchError, Fetcher, HttpFetcher};
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

    // ---- Skills ------------------------------------------------------------

    /// skills.sh's most installed: from memory for six hours, else fetched,
    /// else the copy from the last visit, else the snapshot shipped with the
    /// app. A failed fetch is tried again after a few minutes.
    async fn popular(&self) -> (Vec<SkillsShEntry>, Option<FetchError>) {
        let mut popular = self.state.popular.lock().await;
        if let Some((expires, list)) = popular.as_ref() {
            if Instant::now() < *expires {
                return (list.clone(), None);
            }
        }
        let cache = self.cache_dir.join("skills.json");
        let fetched = match self
            .fetcher
            .get(format!("{}/", self.skills_url), "text/html", 8 << 20)
            .await
        {
            Ok(body) => skills_sh::extract_popular(&String::from_utf8_lossy(&body))
                .map_err(FetchError)
                .and_then(|list| {
                    if list.is_empty() {
                        Err(FetchError("the list of Skills is empty".to_string()))
                    } else {
                        Ok(list)
                    }
                }),
            Err(error) => Err(error),
        };
        match fetched {
            Ok(list) => {
                write_json(&cache, &list);
                *popular = Some((Instant::now() + POPULAR_FOR, list.clone()));
                (list, None)
            }
            Err(error) => {
                let list = read_json::<Vec<SkillsShEntry>>(&cache)
                    .filter(|list| !list.is_empty())
                    .unwrap_or_else(skills_sh::snapshot);
                *popular = Some((Instant::now() + POPULAR_RETRY_AFTER, list.clone()));
                (list, Some(error))
            }
        }
    }

    async fn search_skills(&self, query: &str) -> Result<Vec<SkillsShEntry>, FetchError> {
        if let Some((at, found)) = lock(&self.state.skill_searches).get(query) {
            if at.elapsed() < SEARCH_FOR {
                return Ok(found.clone());
            }
        }
        let url = url::Url::parse_with_params(
            &format!("{}/api/search", self.skills_url),
            &[("q", query), ("limit", "40")],
        )
        .map_err(|error| FetchError(error.to_string()))?;
        let body = self
            .fetcher
            .get(url.to_string(), "application/json", 8 << 20)
            .await?;
        let found = skills_sh::parse_search(&body).map_err(FetchError)?;
        lock(&self.state.skill_searches).insert(query.to_string(), (Instant::now(), found.clone()));
        Ok(found)
    }

    /// Popular Skills matching the query, then what skills.sh finds for it.
    pub(crate) async fn skills(&self, query: &str) -> (Vec<SkillsShEntry>, Option<AppError>) {
        let query = query.trim().to_lowercase();
        let (popular, popular_error) = self.popular().await;
        let mut list = popular
            .into_iter()
            .filter(|entry| {
                query.is_empty()
                    || format!("{} {} {}", entry.name, entry.source, entry.skill_id)
                        .to_lowercase()
                        .contains(&query)
            })
            .collect::<Vec<_>>();
        if query.is_empty() {
            list = skills_sh::spread(list, PER_SOURCE_ON_FIRST_SCREEN);
        }
        let mut error = popular_error;
        if query.chars().count() >= 2 {
            match self.search_skills(&query).await {
                Ok(found) => {
                    for entry in found {
                        if !list.iter().any(|known| {
                            known.source.eq_ignore_ascii_case(&entry.source)
                                && known.skill_id == entry.skill_id
                        }) {
                            list.push(entry);
                        }
                    }
                }
                Err(search_error) => error = Some(search_error),
            }
        }
        list.truncate(MAX_SKILLS);
        let error = error.map(|error| {
            AppError::new(ErrorCode::NetworkError, "error.discover.skillsUnreachable")
                .with_technical(error.0)
                .with_remediation("error.remediation.checkInternetConnection")
        });
        (list, error)
    }

    pub(crate) async fn skill_list(
        &self,
        query: &str,
        holdings: &[SkillHolding],
    ) -> Result<DiscoverSkillList, AppError> {
        validate_query(query)?;
        let (entries, source_error) = self.skills(query).await;
        let known = self.known_descriptions();
        let items = entries
            .into_iter()
            .map(|entry| {
                let id = entry.id();
                let owner = entry.source.split('/').next().unwrap_or_default();
                DiscoverSkill {
                    added: matching::added_skill(
                        &entry.source,
                        &entry.skill_id,
                        &entry.name,
                        holdings,
                    ),
                    description: known.get(&id).cloned(),
                    icon: owner_avatar(owner),
                    id,
                    source: entry.source,
                    skill_id: entry.skill_id,
                    name: entry.name,
                    installs: entry.installs,
                    official: entry.official,
                }
            })
            .collect();
        Ok(DiscoverSkillList {
            items,
            source_error,
        })
    }

    fn known_descriptions(&self) -> HashMap<String, String> {
        let mut known = lock(&self.state.descriptions);
        known
            .get_or_insert_with(|| {
                read_json(&self.cache_dir.join("descriptions.json")).unwrap_or_default()
            })
            .clone()
    }

    /// What each Skill says of itself, read from its skills.sh page, six at a
    /// time, and kept on disk once known. Ids that are malformed or whose page
    /// cannot be read are left out.
    pub async fn descriptions(&self, ids: Vec<String>) -> BTreeMap<String, String> {
        let known = self.known_descriptions();
        let mut out = BTreeMap::new();
        let mut wanted = Vec::new();
        for id in ids.into_iter().take(MAX_DESCRIBED) {
            if let Some(description) = known.get(&id) {
                out.insert(id, description.clone());
            } else if let Some((source, skill_id)) = skills_sh::split_id(&id) {
                let page = format!("{}/{source}/{skill_id}", self.skills_url);
                wanted.push((id, page));
            }
        }
        let fetched = stream::iter(wanted)
            .map(|(id, page)| {
                let fetcher = self.fetcher.clone();
                async move {
                    let body = fetcher.get(page, "text/html", 4 << 20).await.ok()?;
                    let description = skills_sh::meta_description(&String::from_utf8_lossy(&body))?;
                    Some((id, description))
                }
            })
            .buffer_unordered(DESCRIBE_AT_ONCE)
            .filter_map(|found| async move { found })
            .collect::<Vec<_>>()
            .await;
        if !fetched.is_empty() {
            let mut known = lock(&self.state.descriptions);
            let all = known.get_or_insert_with(HashMap::new);
            for (id, description) in &fetched {
                all.insert(id.clone(), description.clone());
            }
            write_json(&self.cache_dir.join("descriptions.json"), all);
        }
        out.extend(fetched);
        out
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
