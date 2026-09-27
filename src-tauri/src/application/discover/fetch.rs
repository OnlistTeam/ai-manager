//! Getting a page, an answer or a picture for the Discover section, and
//! keeping it on disk.
//!
//! Every request goes through the product's outbound client, so the proxy
//! choice in Settings (follow the system, direct, or a custom address,
//! ADR-0056) applies. Nothing but the search words leaves the machine: no
//! key, no cookie, no identifier.

use std::path::{Path, PathBuf};
use std::time::Duration;

use futures::future::BoxFuture;
use serde::de::DeserializeOwned;
use serde::Serialize;

use crate::compat::ccswitch::network_proxy::http_client_for_fixed_url;

const TIMEOUT: Duration = Duration::from_secs(20);

/// skills.sh answers a client it does not recognise with a page of its own,
/// so requests say they come from a browser.
const USER_AGENT: &str = "Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/537.36 \
     (KHTML, like Gecko) Chrome/140.0.0.0 Safari/537.36";

/// A failed request, described without the full address.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct FetchError(pub String);

pub(crate) type FetchResult = Result<Vec<u8>, FetchError>;

/// The one network seam, so tests answer from fixtures.
pub(crate) trait Fetcher: Send + Sync {
    fn get(
        &self,
        url: String,
        accept: &'static str,
        limit: usize,
    ) -> BoxFuture<'static, FetchResult>;
}

pub(crate) struct HttpFetcher;

impl Fetcher for HttpFetcher {
    fn get(
        &self,
        url: String,
        accept: &'static str,
        limit: usize,
    ) -> BoxFuture<'static, FetchResult> {
        Box::pin(async move { fetch(&url, accept, limit).await })
    }
}

fn host_of(url: &str) -> String {
    url::Url::parse(url)
        .ok()
        .and_then(|parsed| parsed.host_str().map(str::to_string))
        .unwrap_or_else(|| "the server".to_string())
}

async fn fetch(url: &str, accept: &'static str, limit: usize) -> FetchResult {
    let host = host_of(url);
    let client = http_client_for_fixed_url(url);
    let mut response = client
        .get(url)
        .header("User-Agent", USER_AGENT)
        .header("Accept", accept)
        // The shared client does not decompress, so ask for none.
        .header("Accept-Encoding", "identity")
        .timeout(TIMEOUT)
        .send()
        .await
        .map_err(|error| {
            let reason = if error.is_timeout() {
                "timed out"
            } else if error.is_connect() {
                "could not connect"
            } else {
                "request failed"
            };
            FetchError(format!("{host}: {reason}"))
        })?;
    if !response.status().is_success() {
        return Err(FetchError(format!(
            "{host} answered {}",
            response.status().as_u16()
        )));
    }
    let mut body = Vec::new();
    while let Some(chunk) = response
        .chunk()
        .await
        .map_err(|_| FetchError(format!("{host}: the answer was cut off")))?
    {
        if body.len() + chunk.len() > limit {
            return Err(FetchError(format!("{host}: the answer is too large")));
        }
        body.extend_from_slice(&chunk);
    }
    Ok(body)
}

/// Where the Discover section keeps what it fetched, under the product's data
/// directory.
pub(crate) fn default_cache_dir() -> PathBuf {
    crate::infrastructure::paths::product_data_dir()
        .join("cache")
        .join("discover")
}

pub(crate) fn read_json<T: DeserializeOwned>(path: &Path) -> Option<T> {
    let bytes = std::fs::read(path).ok()?;
    serde_json::from_slice(&bytes).ok()
}

/// Written beside and renamed over, so a crash never leaves half a file.
pub(crate) fn write_file(path: &Path, bytes: &[u8]) {
    let Some(parent) = path.parent() else {
        return;
    };
    if std::fs::create_dir_all(parent).is_err() {
        return;
    }
    let temporary = path.with_extension("partial");
    if std::fs::write(&temporary, bytes).is_ok() && std::fs::rename(&temporary, path).is_err() {
        let _ = std::fs::remove_file(&temporary);
    }
}

pub(crate) fn write_json<T: Serialize>(path: &Path, value: &T) {
    if let Ok(bytes) = serde_json::to_vec(value) {
        write_file(path, &bytes);
    }
}
