//! The Skill half of the Discover section: skills.sh's most installed, its
//! search, and what each Skill says of itself.

use std::collections::{BTreeMap, HashMap};
use std::time::Instant;

use futures::stream::{self, StreamExt};

use super::catalog::owner_avatar;
use super::fetch::{read_json, write_json, FetchError};
use super::skills_sh::{self, SkillsShEntry};
use super::{
    lock, matching, validate_query, DiscoverService, DESCRIBE_AT_ONCE, MAX_DESCRIBED, MAX_SKILLS,
    PER_SOURCE_ON_FIRST_SCREEN, POPULAR_FOR, POPULAR_RETRY_AFTER, SEARCH_FOR,
};
use crate::compat::ccswitch::discover::SkillHolding;
use crate::domain::{AppError, DiscoverSkill, DiscoverSkillList, ErrorCode};

impl DiscoverService {
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
}
