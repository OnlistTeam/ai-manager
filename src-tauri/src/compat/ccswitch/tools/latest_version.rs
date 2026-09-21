//! The publisher index a proven owner really installs from.
//!
//! Upstream inventory fetches one "latest" per tool regardless of who owns
//! the launcher. A uv/pipx-owned Hermes can only install what PyPI publishes,
//! so its status and update target must follow the PyPI catalog; every other
//! owner keeps the upstream answer (the npm registry for npm-compatible tools,
//! the official checkout's own channel for an unmanaged Hermes).

use std::time::Duration;

use crate::commands::misc::ToolVersion;
use crate::compat::ccswitch::install_probe::LifecycleProbe;
use crate::compat::ccswitch::versioning::{
    catalog_target, pypi, source_for_probe, VersionCatalogRequest,
};
use crate::domain::ToolId;

const OWNER_INDEX_TIMEOUT: Duration = Duration::from_secs(30);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum LatestVersionIndex {
    Upstream,
    HermesPypi,
}

/// Derived from the same catalog decision the version directory uses, so the
/// status, the update target and the selectable versions agree on one index.
pub(crate) fn latest_version_index(id: ToolId, probe: &LifecycleProbe) -> LatestVersionIndex {
    match catalog_target(id, probe)
        .ok()
        .and_then(|target| target.request)
    {
        Some(VersionCatalogRequest::HermesPypi) => LatestVersionIndex::HermesPypi,
        Some(VersionCatalogRequest::Npm(_)) | None => LatestVersionIndex::Upstream,
    }
}

pub(crate) async fn align_with_owner(
    id: ToolId,
    probe: &LifecycleProbe,
    version: &mut ToolVersion,
) {
    if latest_version_index(id, probe) != LatestVersionIndex::HermesPypi {
        return;
    }
    let latest = pypi::fetch_catalog(id, source_for_probe(probe), OWNER_INDEX_TIMEOUT)
        .await
        .ok()
        .and_then(|catalog| catalog.latest_version);
    apply(version, latest);
}

/// A failed owner-index read leaves no latest at all: `derive_status` then
/// reports Installed instead of an update the owner cannot perform.
fn apply(version: &mut ToolVersion, owner_latest: Option<String>) {
    version.latest_version = owner_latest;
}

#[cfg(test)]
mod tests {
    use super::{apply, latest_version_index, LatestVersionIndex};
    use crate::commands::misc::ToolVersion;
    use crate::compat::ccswitch::install_probe::{
        HermesInstallOwner, InstallSource, InstalledEntry, LifecycleProbe,
    };
    use crate::compat::ccswitch::tools::tool_from_version;
    use crate::domain::{ToolId, ToolStatus};
    use std::path::PathBuf;

    fn hermes(source: InstallSource, owner: Option<HermesInstallOwner>) -> LifecycleProbe {
        LifecycleProbe {
            entry: Some(InstalledEntry {
                bin_path: PathBuf::from("/Users/a/.local/bin/hermes"),
                real_path: PathBuf::from("/Users/a/.local/bin/hermes"),
                source,
                brew_formula: None,
                runnable: true,
                npm_package: None,
                hermes_owner: owner,
            }),
            path_env: None,
        }
    }

    #[test]
    fn only_a_proven_python_owner_reads_pypi() {
        let uv = HermesInstallOwner::Uv {
            program_path: PathBuf::from("/Users/a/.local/bin/uv"),
        };
        assert_eq!(
            latest_version_index(
                ToolId::Hermes,
                &hermes(InstallSource::UvTool, Some(uv.clone()))
            ),
            LatestVersionIndex::HermesPypi
        );
        assert_eq!(
            latest_version_index(
                ToolId::Hermes,
                &hermes(
                    InstallSource::Pipx,
                    Some(HermesInstallOwner::Pipx {
                        program_path: PathBuf::from("/usr/bin/pipx"),
                        global: true,
                    })
                )
            ),
            LatestVersionIndex::HermesPypi
        );
        // A source tag without its matching receipt is not ownership.
        assert_eq!(
            latest_version_index(ToolId::Hermes, &hermes(InstallSource::Pipx, Some(uv))),
            LatestVersionIndex::Upstream
        );
        assert_eq!(
            latest_version_index(ToolId::Hermes, &hermes(InstallSource::Unmanaged, None)),
            LatestVersionIndex::Upstream
        );
        assert_eq!(
            latest_version_index(ToolId::Hermes, &LifecycleProbe::default()),
            LatestVersionIndex::Upstream
        );
        let mut grok = hermes(InstallSource::NodeManagerNpm, None);
        grok.entry.as_mut().unwrap().npm_package = Some("@xai-official/grok");
        assert_eq!(
            latest_version_index(ToolId::GrokBuild, &grok),
            LatestVersionIndex::Upstream
        );
    }

    /// PyPI froze at 0.19.0 while GitHub releases moved on; a uv-owned install
    /// must not advertise an update `uv tool install hermes-agent@latest`
    /// cannot deliver, and a failed PyPI read must stay conservative.
    #[test]
    fn the_status_follows_the_index_the_owner_installs_from() {
        let mut version = ToolVersion {
            name: "hermes".to_string(),
            version: Some("0.19.0".to_string()),
            latest_version: Some("0.21.0".to_string()),
            error: None,
            installed_but_broken: false,
            env_type: "macos".to_string(),
            wsl_distro: None,
        };
        assert_eq!(
            tool_from_version(ToolId::Hermes, &version).status,
            ToolStatus::UpdateAvailable
        );
        apply(&mut version, Some("0.19.0".to_string()));
        let aligned = tool_from_version(ToolId::Hermes, &version);
        assert_eq!(aligned.status, ToolStatus::Installed);
        assert_eq!(aligned.latest_version.as_deref(), Some("0.19.0"));
        apply(&mut version, Some("0.20.0".to_string()));
        assert_eq!(
            tool_from_version(ToolId::Hermes, &version).status,
            ToolStatus::UpdateAvailable
        );
        apply(&mut version, None);
        assert_eq!(
            tool_from_version(ToolId::Hermes, &version).status,
            ToolStatus::Installed
        );
    }
}
