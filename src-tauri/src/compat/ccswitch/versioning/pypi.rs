use std::cmp::Ordering;

use crate::commands::misc::compare_semver;
use crate::domain::{
    validate_tool_version, AppError, ToolId, ToolInstallSource, ToolVersionCatalog, ToolVersionTag,
};

use super::{catalog_failed, MAX_CATALOG_BYTES};

pub(crate) const HERMES_PYPI_URL: &str = "https://pypi.org/pypi/hermes-agent/json";

/// Keep the inherited proxy client behind the upstream compatibility boundary
/// and expose only the fixed Hermes package endpoint.
fn request() -> reqwest::RequestBuilder {
    crate::proxy::http_client::get_for_url(HERMES_PYPI_URL).get(HERMES_PYPI_URL)
}

/// Fetches the fixed Hermes endpoint through the product client and applies
/// the ADR-0020 checks: HTTPS endpoint, HTTP status, JSON content type, a
/// 1 MiB streamed bound and UTF-8, before the catalog itself is validated.
pub(crate) async fn fetch_catalog(
    id: ToolId,
    source: ToolInstallSource,
    timeout: std::time::Duration,
) -> Result<ToolVersionCatalog, AppError> {
    use reqwest::header::{ACCEPT, CONTENT_TYPE};

    let mut response = request()
        .header(ACCEPT, "application/json")
        .timeout(timeout)
        .send()
        .await
        .map_err(|error| catalog_failed(format!("PyPI request failed: {error}")))?;
    if !response.status().is_success() {
        return Err(catalog_failed(format!(
            "PyPI returned HTTP {}",
            response.status().as_u16()
        )));
    }
    let content_type = response
        .headers()
        .get(CONTENT_TYPE)
        .and_then(|value| value.to_str().ok())
        .unwrap_or_default();
    if !content_type
        .split(';')
        .next()
        .is_some_and(|mime| mime.trim().eq_ignore_ascii_case("application/json"))
    {
        return Err(catalog_failed("PyPI returned a non-JSON content type"));
    }
    if response
        .content_length()
        .is_some_and(|length| length > MAX_CATALOG_BYTES as u64)
    {
        return Err(catalog_failed("PyPI response exceeded 1 MiB"));
    }
    let mut body = Vec::new();
    while let Some(chunk) = response
        .chunk()
        .await
        .map_err(|error| catalog_failed(format!("PyPI response read failed: {error}")))?
    {
        if body.len().saturating_add(chunk.len()) > MAX_CATALOG_BYTES {
            return Err(catalog_failed("PyPI response exceeded 1 MiB"));
        }
        body.extend_from_slice(&chunk);
    }
    let body = String::from_utf8(body)
        .map_err(|error| catalog_failed(format!("PyPI response was not UTF-8: {error}")))?;
    parse_hermes_catalog(id, source, &body)
}

pub(crate) fn parse_hermes_catalog(
    id: ToolId,
    source: ToolInstallSource,
    stdout: &str,
) -> Result<ToolVersionCatalog, AppError> {
    if id != ToolId::Hermes || !matches!(source, ToolInstallSource::Uv | ToolInstallSource::Pipx) {
        return Err(catalog_failed(
            "PyPI catalog requested without a proven Hermes owner",
        ));
    }
    if stdout.len() > MAX_CATALOG_BYTES {
        return Err(catalog_failed("PyPI response exceeded 1 MiB"));
    }
    let root: serde_json::Value =
        serde_json::from_str(stdout).map_err(|error| catalog_failed(error.to_string()))?;
    let info = root
        .get("info")
        .and_then(serde_json::Value::as_object)
        .ok_or_else(|| catalog_failed("PyPI response omitted info"))?;
    if info
        .get("name")
        .and_then(serde_json::Value::as_str)
        .is_none_or(|name| python_name(name) != "hermes-agent")
    {
        return Err(catalog_failed(
            "PyPI response package name did not match hermes-agent",
        ));
    }
    let latest = info
        .get("version")
        .and_then(serde_json::Value::as_str)
        .and_then(|version| validate_tool_version(version).ok());
    let releases = root
        .get("releases")
        .and_then(serde_json::Value::as_object)
        .ok_or_else(|| catalog_failed("PyPI response omitted releases"))?;
    let mut versions = releases
        .iter()
        .filter(|(_, files)| {
            files
                .as_array()
                .is_some_and(|artifacts| !artifacts.is_empty())
        })
        .filter_map(|(version, _)| validate_tool_version(version).ok())
        .collect::<Vec<_>>();
    if let Some(latest) = latest.as_ref() {
        versions.push(latest.clone());
    }
    versions.sort_by(|left, right| {
        compare_semver(right, left)
            .unwrap_or_else(|| right.cmp(left))
            .then(Ordering::Equal)
    });
    versions.dedup();
    versions.truncate(crate::domain::tool_version::MAX_TOOL_VERSION_CATALOG);
    if versions.is_empty() {
        return Err(catalog_failed("PyPI returned no installable safe versions"));
    }
    let dist_tags = latest
        .as_ref()
        .map(|version| ToolVersionTag {
            tag: "latest".to_string(),
            version: version.clone(),
        })
        .into_iter()
        .collect();
    Ok(ToolVersionCatalog {
        tool: id,
        source,
        can_change_version: true,
        restriction: None,
        latest_version: latest,
        dist_tags,
        versions,
        mirror_used: false,
    })
}

fn python_name(raw: &str) -> String {
    let mut normalized = String::with_capacity(raw.len());
    let mut separator = false;
    for character in raw.chars() {
        if matches!(character, '-' | '_' | '.') {
            if !separator {
                normalized.push('-');
                separator = true;
            }
        } else {
            normalized.push(character.to_ascii_lowercase());
            separator = false;
        }
    }
    normalized
}

#[cfg(test)]
mod tests {
    use super::parse_hermes_catalog;
    use crate::domain::{ToolId, ToolInstallSource, ToolVersionTag};

    #[test]
    fn pypi_catalog_uses_publisher_latest_and_only_releases_with_files() {
        let catalog = parse_hermes_catalog(
            ToolId::Hermes,
            ToolInstallSource::Uv,
            r#"{
                "info":{"name":"Hermes_Agent","version":"0.19.0"},
                "releases":{
                    "0.20.0rc1":[{"filename":"candidate.whl"}],
                    "0.19.0":[{"filename":"stable.whl"}],
                    "0.18.0":[{"filename":"old.whl"}],
                    "0.17.0":[],
                    "bad version":[{"filename":"bad.whl"}]
                }
            }"#,
        )
        .unwrap();
        assert_eq!(catalog.latest_version.as_deref(), Some("0.19.0"));
        assert_eq!(catalog.versions, vec!["0.20.0rc1", "0.19.0", "0.18.0"]);
        assert_eq!(
            catalog.dist_tags,
            vec![ToolVersionTag {
                tag: "latest".to_string(),
                version: "0.19.0".to_string()
            }]
        );
        assert!(!catalog.mirror_used);
    }

    #[test]
    fn malformed_wrong_package_and_unproven_sources_fail_closed() {
        assert!(parse_hermes_catalog(ToolId::Hermes, ToolInstallSource::Pipx, "not json").is_err());
        assert!(parse_hermes_catalog(
            ToolId::Hermes,
            ToolInstallSource::Pipx,
            r#"{"info":{"name":"other","version":"1.0.0"},"releases":{"1.0.0":[{}]}}"#
        )
        .is_err());
        assert!(parse_hermes_catalog(
            ToolId::Hermes,
            ToolInstallSource::Unmanaged,
            r#"{"info":{"name":"hermes-agent","version":"1.0.0"},"releases":{"1.0.0":[{}]}}"#
        )
        .is_err());
    }
}
