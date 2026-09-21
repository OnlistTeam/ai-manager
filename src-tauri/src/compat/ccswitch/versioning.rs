use std::cmp::Ordering;

use crate::commands::misc::{compare_semver, highest_stable_version, npm_package_for};
use crate::compat::ccswitch::install_probe::{
    HermesInstallOwner, InstallSource, InstalledEntry, LifecycleProbe,
};
use crate::compat::ccswitch::lifecycle_specs::{
    anchored, package_manager_install_version, path_npm_spec, INSTALL_TIMEOUT,
};
use crate::compat::ccswitch::tools::tool_id_to_cli_name;
use crate::domain::{
    validate_tool_version, AppError, ErrorCode, ToolId, ToolInstallSource, ToolVersionCatalog,
    ToolVersionRestriction, ToolVersionTag,
};
use crate::platform::command::{AllowedProgram, CommandSpec};
use crate::platform::plan::LifecyclePlan;

pub(crate) mod pypi;

const CATALOG_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(120);
pub(crate) const MAX_CATALOG_BYTES: usize = 1_048_576;

#[derive(Clone)]
pub enum VersionCatalogRequest {
    /// An npm-registry `view` through the owner's client (npm or pnpm).
    Npm(CommandSpec),
    HermesPypi,
}

pub struct VersionCatalogTarget {
    pub source: ToolInstallSource,
    pub restriction: Option<ToolVersionRestriction>,
    pub request: Option<VersionCatalogRequest>,
}

/// The proven owner category of the selected launcher. A source tag without
/// its matching receipt evidence is not ownership.
pub fn source_for_probe(probe: &LifecycleProbe) -> ToolInstallSource {
    match probe.entry.as_ref() {
        None => ToolInstallSource::NotInstalled,
        Some(entry) => match (&entry.source, entry.hermes_owner.as_ref()) {
            (InstallSource::NodeManagerNpm, _) if entry.npm_package.is_some() => {
                ToolInstallSource::Npm
            }
            (InstallSource::Pnpm, _) if entry.npm_package.is_some() => ToolInstallSource::Pnpm,
            (InstallSource::Bun, _) if entry.npm_package.is_some() => ToolInstallSource::Bun,
            (InstallSource::Volta, _) if entry.npm_package.is_some() => ToolInstallSource::Volta,
            (InstallSource::UvTool, Some(HermesInstallOwner::Uv { .. })) => ToolInstallSource::Uv,
            (InstallSource::Pipx, Some(HermesInstallOwner::Pipx { .. })) => ToolInstallSource::Pipx,
            (
                InstallSource::NodeManagerNpm
                | InstallSource::Pnpm
                | InstallSource::Bun
                | InstallSource::Volta
                | InstallSource::UvTool
                | InstallSource::Pipx,
                _,
            ) => ToolInstallSource::Unmanaged,
            (InstallSource::Brew | InstallSource::BrewCask, _) => ToolInstallSource::Brew,
            (InstallSource::Native, _) => ToolInstallSource::NativeInstaller,
            (InstallSource::Unmanaged, _) => ToolInstallSource::Unmanaged,
        },
    }
}

pub fn catalog_target(
    id: ToolId,
    probe: &LifecycleProbe,
) -> Result<VersionCatalogTarget, AppError> {
    let source = source_for_probe(probe);
    if id == ToolId::Hermes {
        let proven_owner = probe.entry.as_ref().is_some_and(hermes_owner_matches);
        return Ok(VersionCatalogTarget {
            source,
            restriction: (!proven_owner).then_some(ToolVersionRestriction::Unmanaged),
            request: proven_owner.then_some(VersionCatalogRequest::HermesPypi),
        });
    }

    let package = package(id)?;
    let restriction = restriction(source);
    let request = restriction
        .is_none()
        .then(|| VersionCatalogRequest::Npm(catalog_spec(package, probe)));
    Ok(VersionCatalogTarget {
        source,
        restriction,
        request,
    })
}

/// The registry client of the owner that will install the chosen version.
/// npm-managed and Volta launchers have `npm` beside them (Volta shims it);
/// pnpm answers `pnpm view` in npm's JSON shape. Bun has no registry query
/// that works outside a project (`bun info` needs a `package.json` in the
/// working directory), so Bun-owned and uninstalled tools ask the PATH npm.
fn catalog_spec(package: &str, probe: &LifecycleProbe) -> CommandSpec {
    let args = vec![
        "view".to_string(),
        package.to_string(),
        "dist-tags".to_string(),
        "versions".to_string(),
        "--json".to_string(),
    ];
    let owner = probe.entry.as_ref().and_then(|entry| match entry.source {
        InstallSource::NodeManagerNpm | InstallSource::Volta => {
            anchored(AllowedProgram::Npm, entry, "npm", args.clone(), probe, true)
        }
        InstallSource::Pnpm => anchored(
            AllowedProgram::Pnpm,
            entry,
            "pnpm",
            args.clone(),
            probe,
            false,
        ),
        InstallSource::Bun
        | InstallSource::UvTool
        | InstallSource::Pipx
        | InstallSource::Brew
        | InstallSource::BrewCask
        | InstallSource::Native
        | InstallSource::Unmanaged => None,
    });
    owner
        .unwrap_or_else(|| path_npm_spec(args, probe))
        .with_timeout(CATALOG_TIMEOUT)
}

fn hermes_owner_matches(entry: &InstalledEntry) -> bool {
    matches!(
        (&entry.source, entry.hermes_owner.as_ref()),
        (InstallSource::UvTool, Some(HermesInstallOwner::Uv { .. }))
            | (InstallSource::Pipx, Some(HermesInstallOwner::Pipx { .. }))
    )
}

pub fn install_version_plan(
    id: ToolId,
    probe: &LifecycleProbe,
    raw_version: &str,
) -> Result<LifecyclePlan, AppError> {
    let version = validate_tool_version(raw_version)?;
    if id == ToolId::Hermes {
        let entry = probe.entry.as_ref().ok_or_else(|| {
            AppError::new(
                ErrorCode::UpdateFailed,
                "error.tool.versionSourceUnsupported",
            )
            .with_technical("Hermes has no proven uv/pipx owner")
            .with_remediation("error.remediation.useOriginalUpdateChannel")
        })?;
        let spec = crate::compat::ccswitch::lifecycle_specs::hermes_python::install_version(
            entry, probe, &version,
        )
        .ok_or_else(|| source_unsupported(id, entry.source))?;
        return Ok(LifecyclePlan::single(spec));
    }
    let package = package(id)?;
    let spec = match probe.entry.as_ref() {
        None => path_npm_spec(
            vec![
                "install".to_string(),
                "-g".to_string(),
                format!("{package}@{version}"),
            ],
            probe,
        )
        .with_timeout(INSTALL_TIMEOUT),
        Some(entry) => package_manager_install_version(entry, probe, &version)
            .ok_or_else(|| source_unsupported(id, entry.source))?,
    };
    Ok(LifecyclePlan::single(spec))
}

pub fn blocked_catalog(id: ToolId, target: &VersionCatalogTarget) -> ToolVersionCatalog {
    ToolVersionCatalog {
        tool: id,
        source: target.source,
        can_change_version: false,
        restriction: target.restriction,
        latest_version: None,
        dist_tags: Vec::new(),
        versions: Vec::new(),
        mirror_used: false,
    }
}

pub fn parse_catalog(
    id: ToolId,
    source: ToolInstallSource,
    stdout: &str,
    mirror_used: bool,
) -> Result<ToolVersionCatalog, AppError> {
    if stdout.len() > MAX_CATALOG_BYTES {
        return Err(catalog_failed("npm registry response exceeded 1 MiB"));
    }
    let value: serde_json::Value =
        serde_json::from_str(stdout).map_err(|error| catalog_failed(error.to_string()))?;
    let registry_latest = value
        .get("dist-tags")
        .and_then(serde_json::Value::as_object)
        .and_then(|tags| tags.get("latest"))
        .and_then(serde_json::Value::as_str)
        // Keep parsing the legacy response shape for compatibility with older
        // mirrors while every request now explicitly asks npm for dist-tags.
        .or_else(|| value.get("version").and_then(serde_json::Value::as_str))
        .or_else(|| value.as_str())
        .and_then(|version| validate_tool_version(version).ok());
    let mut dist_tags = value
        .get("dist-tags")
        .and_then(serde_json::Value::as_object)
        .into_iter()
        .flat_map(|tags| tags.iter())
        .filter(|(tag, _)| safe_dist_tag(tag))
        .filter_map(|(tag, version)| {
            validate_tool_version(version.as_str()?)
                .ok()
                .map(|version| ToolVersionTag {
                    tag: tag.clone(),
                    version,
                })
        })
        .collect::<Vec<_>>();
    dist_tags.sort_by(|left, right| {
        (left.tag != "latest")
            .cmp(&(right.tag != "latest"))
            .then_with(|| left.tag.cmp(&right.tag))
    });
    dist_tags.truncate(crate::domain::tool_version::MAX_TOOL_VERSION_TAGS);
    let raw_versions = value
        .get("versions")
        .and_then(serde_json::Value::as_array)
        .or_else(|| value.as_array());

    let mut versions = raw_versions
        .into_iter()
        .flatten()
        .filter_map(serde_json::Value::as_str)
        .filter_map(|version| validate_tool_version(version).ok())
        .collect::<Vec<_>>();
    if let Some(latest) = registry_latest.as_ref() {
        versions.push(latest.clone());
    }
    // Grok's npm `latest` tag has lagged behind its published stable releases
    // in production. Ordinary software management should select the newest
    // stable build automatically; the full catalog remains available for
    // intentional compatibility switches and rollbacks.
    let latest = if id == ToolId::GrokBuild {
        highest_stable_version(versions.iter().map(String::as_str))
            .or_else(|| registry_latest.clone())
    } else {
        registry_latest
    };
    versions.sort_by(|left, right| {
        compare_semver(right, left)
            .unwrap_or_else(|| right.cmp(left))
            .then(Ordering::Equal)
    });
    versions.dedup();
    versions.truncate(crate::domain::tool_version::MAX_TOOL_VERSION_CATALOG);
    if versions.is_empty() {
        return Err(catalog_failed("npm registry returned no safe versions"));
    }

    Ok(ToolVersionCatalog {
        tool: id,
        source,
        can_change_version: true,
        restriction: None,
        latest_version: latest,
        dist_tags,
        versions,
        mirror_used,
    })
}

fn safe_dist_tag(tag: &str) -> bool {
    !tag.is_empty()
        && tag.len() <= crate::domain::tool_version::MAX_TOOL_VERSION_TAG_LENGTH
        && tag.is_ascii()
        && tag
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.'))
}

fn package(id: ToolId) -> Result<&'static str, AppError> {
    npm_package_for(tool_id_to_cli_name(id)).ok_or_else(|| {
        AppError::new(ErrorCode::UpdateFailed, "error.tool.actionUnsupported")
            .with_technical(format!("{} has no npm package mapping", id.as_str()))
            .with_remediation("error.remediation.installManually")
    })
}

fn restriction(source: ToolInstallSource) -> Option<ToolVersionRestriction> {
    match source {
        ToolInstallSource::NativeInstaller => Some(ToolVersionRestriction::NativeInstaller),
        ToolInstallSource::Brew => Some(ToolVersionRestriction::Brew),
        ToolInstallSource::Unmanaged => Some(ToolVersionRestriction::Unmanaged),
        ToolInstallSource::NotInstalled
        | ToolInstallSource::Npm
        | ToolInstallSource::Pnpm
        | ToolInstallSource::Bun
        | ToolInstallSource::Volta
        | ToolInstallSource::Uv
        | ToolInstallSource::Pipx => None,
    }
}

fn source_unsupported(id: ToolId, source: InstallSource) -> AppError {
    AppError::new(
        ErrorCode::UpdateFailed,
        "error.tool.versionSourceUnsupported",
    )
    .with_technical(format!("{} is owned by {source:?}", id.as_str()))
    .with_remediation("error.remediation.useOriginalUpdateChannel")
}

/// Semver-style comparison for product layers that may not reach into the
/// inherited command module. `None` when either side is not a version.
pub(crate) fn compare_versions(left: &str, right: &str) -> Option<Ordering> {
    compare_semver(left, right)
}

pub(crate) fn catalog_failed(technical: impl Into<String>) -> AppError {
    AppError::new(ErrorCode::UpdateFailed, "error.tool.versionCatalogFailed")
        .with_technical(technical)
        .with_remediation("error.remediation.retryOrViewDetails")
}

#[cfg(test)]
mod tests {
    use super::source_for_probe;
    use super::{catalog_target, install_version_plan, parse_catalog, VersionCatalogRequest};
    use crate::compat::ccswitch::install_probe::{
        HermesInstallOwner, InstallSource, InstalledEntry, LifecycleProbe,
    };
    use crate::domain::{ToolId, ToolInstallSource, ToolVersionRestriction, ToolVersionTag};
    use crate::platform::command::AllowedProgram;
    use std::path::PathBuf;

    /// Anchoring asks the file system for the owner's launcher on Windows (`npm.cmd`, never
    /// a bare `npm`), so the anchored expectations are built from a real directory instead
    /// of a synthetic POSIX path.
    struct OwnerDir(tempfile::TempDir);

    impl OwnerDir {
        fn with(programs: &[&str]) -> Self {
            let temp = tempfile::tempdir().expect("owner directory");
            for program in programs {
                std::fs::write(temp.path().join(launcher(program)), b"").expect("owner launcher");
            }
            Self(temp)
        }

        fn path(&self, program: &str) -> PathBuf {
            self.0.path().join(launcher(program))
        }

        fn root(&self) -> &std::path::Path {
            self.0.path()
        }
    }

    fn launcher(program: &str) -> String {
        if cfg!(target_os = "windows") {
            format!("{program}.cmd")
        } else {
            program.to_string()
        }
    }

    fn owned_probe(source: InstallSource, dir: &OwnerDir) -> LifecycleProbe {
        let mut owned = probe(source);
        let entry = owned.entry.as_mut().expect("probed entry");
        entry.bin_path = dir.path("claude");
        entry.real_path = entry.bin_path.clone();
        owned
    }

    fn probe(source: InstallSource) -> LifecycleProbe {
        LifecycleProbe {
            entry: Some(InstalledEntry {
                bin_path: PathBuf::from("/Users/a/.nvm/versions/node/v22/bin/claude"),
                real_path: PathBuf::from("/Users/a/.nvm/versions/node/v22/bin/claude"),
                source,
                brew_formula: Some("claude-code".to_string()),
                runnable: true,
                npm_package: Some("@anthropic-ai/claude-code"),
                hermes_owner: None,
            }),
            path_env: Some(("PATH".to_string(), "/usr/bin:/bin".to_string())),
        }
    }

    #[test]
    fn claude_code_npm_version_is_owner_anchored_and_never_shell_interpolated() {
        let owner = OwnerDir::with(&["claude", "npm"]);
        let plan = install_version_plan(
            ToolId::ClaudeCode,
            &owned_probe(InstallSource::NodeManagerNpm, &owner),
            "1.2.3-beta.1",
        )
        .unwrap();
        let spec = &plan.attempts[0].steps[0].spec;
        assert_eq!(spec.program, AllowedProgram::Npm);
        assert_eq!(spec.program_path, Some(owner.path("npm")));
        assert_eq!(
            spec.args,
            vec!["install", "-g", "@anthropic-ai/claude-code@1.2.3-beta.1"]
        );
        assert!(spec.validate().is_ok());
    }

    /// A catalog is only useful if the owner that will install from it can be
    /// asked. pnpm answers `pnpm view` in npm's JSON shape and Volta always
    /// shims `npm` beside its launchers; Bun has no registry query that works
    /// outside a project, so it keeps the PATH npm like an uninstalled tool.
    #[test]
    fn the_version_catalog_asks_the_owner_that_can_install_from_it() {
        let view = vec![
            "view",
            "@anthropic-ai/claude-code",
            "dist-tags",
            "versions",
            "--json",
        ];
        let request = |source: InstallSource, owner: &OwnerDir| match catalog_target(
            ToolId::ClaudeCode,
            &owned_probe(source, owner),
        )
        .unwrap()
        .request
        {
            Some(VersionCatalogRequest::Npm(spec)) => spec,
            other => panic!("{source:?} must query a registry, got {}", other.is_some()),
        };

        let pnpm_owner = OwnerDir::with(&["claude", "pnpm"]);
        let pnpm = request(InstallSource::Pnpm, &pnpm_owner);
        assert_eq!(pnpm.program, AllowedProgram::Pnpm);
        assert_eq!(pnpm.program_path, Some(pnpm_owner.path("pnpm")));
        assert_eq!(pnpm.args, view);

        let volta_owner = OwnerDir::with(&["claude", "npm"]);
        let volta = request(InstallSource::Volta, &volta_owner);
        assert_eq!(volta.program, AllowedProgram::Npm);
        assert_eq!(volta.program_path, Some(volta_owner.path("npm")));

        let npm_owner = OwnerDir::with(&["claude", "npm"]);
        let npm = request(InstallSource::NodeManagerNpm, &npm_owner);
        assert_eq!(npm.program_path, Some(npm_owner.path("npm")));

        // Bun has no npm beside it and none on the injected PATH either, so the spec stays
        // bare and the executor reports `programNotFound` truthfully.
        let bun_owner = OwnerDir::with(&["claude", "bun"]);
        let mut bun_probe = owned_probe(InstallSource::Bun, &bun_owner);
        bun_probe.path_env = Some((
            "PATH".to_string(),
            bun_owner.root().to_string_lossy().into_owned(),
        ));
        let bun = match catalog_target(ToolId::ClaudeCode, &bun_probe)
            .unwrap()
            .request
        {
            Some(VersionCatalogRequest::Npm(spec)) => spec,
            _ => panic!("Bun must query a registry"),
        };
        assert_eq!(bun.program, AllowedProgram::Npm);
        assert_eq!(bun.program_path, None);
        assert_eq!(bun.args, view);
    }

    #[test]
    fn native_and_brew_sources_are_visible_but_cannot_cross_to_npm() {
        for (source, expected) in [
            (
                InstallSource::Native,
                ToolVersionRestriction::NativeInstaller,
            ),
            (InstallSource::Brew, ToolVersionRestriction::Brew),
            (InstallSource::BrewCask, ToolVersionRestriction::Brew),
            (InstallSource::Unmanaged, ToolVersionRestriction::Unmanaged),
        ] {
            let target = catalog_target(ToolId::ClaudeCode, &probe(source)).unwrap();
            assert_eq!(target.restriction, Some(expected));
            assert!(target.request.is_none());
            assert!(install_version_plan(ToolId::ClaudeCode, &probe(source), "1.2.3").is_err());
        }
    }

    #[test]
    fn python_sources_require_matching_owner_receipts() {
        let mut uv = probe(InstallSource::UvTool);
        {
            let entry = uv.entry.as_mut().unwrap();
            entry.bin_path = PathBuf::from("/Users/a/.local/bin/hermes");
            entry.real_path = entry.bin_path.clone();
            entry.npm_package = None;
            entry.hermes_owner = Some(HermesInstallOwner::Uv {
                program_path: PathBuf::from("/Users/a/.local/bin/uv"),
            });
        }
        assert_eq!(source_for_probe(&uv), ToolInstallSource::Uv);

        uv.entry.as_mut().unwrap().hermes_owner = Some(HermesInstallOwner::Pipx {
            program_path: PathBuf::from("/Users/a/.local/bin/pipx"),
            global: false,
        });
        assert_eq!(source_for_probe(&uv), ToolInstallSource::Unmanaged);

        uv.entry.as_mut().unwrap().source = InstallSource::Pipx;
        assert_eq!(source_for_probe(&uv), ToolInstallSource::Pipx);

        {
            let entry = uv.entry.as_mut().unwrap();
            entry.source = InstallSource::NodeManagerNpm;
            entry.hermes_owner = None;
        }
        assert_eq!(source_for_probe(&uv), ToolInstallSource::Unmanaged);
        uv.entry.as_mut().unwrap().npm_package = Some("@anthropic-ai/claude-code");
        assert_eq!(source_for_probe(&uv), ToolInstallSource::Npm);
        uv.entry.as_mut().unwrap().source = InstallSource::Native;
        assert_eq!(source_for_probe(&uv), ToolInstallSource::NativeInstaller);
        assert_eq!(
            source_for_probe(&LifecycleProbe::default()),
            ToolInstallSource::NotInstalled
        );
    }

    #[test]
    fn registry_catalog_is_sorted_deduplicated_and_bounded_to_safe_versions() {
        let catalog = parse_catalog(
            ToolId::ClaudeCode,
            ToolInstallSource::Npm,
            r#"{"dist-tags":{"latest":"2.0.0","beta":"2.1.0-beta.1","bad tag":"9.9.9"},"versions":["1.0.0","2.0.0","2.1.0-beta.1","1.5.0","2.0.0","bad tag","x"]}"#,
            true,
        )
        .unwrap();
        assert_eq!(
            catalog.versions,
            vec!["2.1.0-beta.1", "2.0.0", "1.5.0", "1.0.0"]
        );
        assert_eq!(catalog.latest_version.as_deref(), Some("2.0.0"));
        assert_eq!(
            catalog.dist_tags,
            vec![
                ToolVersionTag {
                    tag: "latest".to_string(),
                    version: "2.0.0".to_string(),
                },
                ToolVersionTag {
                    tag: "beta".to_string(),
                    version: "2.1.0-beta.1".to_string(),
                },
            ]
        );
        assert!(catalog.mirror_used);
    }

    #[test]
    fn grok_recommends_highest_published_stable_when_latest_tag_falls_behind() {
        let catalog = parse_catalog(
            ToolId::GrokBuild,
            ToolInstallSource::Npm,
            r#"{"dist-tags":{"latest":"0.1.4","alpha":"1.0.12"},"versions":["1.0.12","1.0.11","1.0.5","0.1.4"]}"#,
            false,
        )
        .unwrap();

        assert_eq!(catalog.latest_version.as_deref(), Some("1.0.12"));
        assert_eq!(catalog.versions, vec!["1.0.12", "1.0.11", "1.0.5", "0.1.4"]);
    }

    #[test]
    fn invalid_version_cannot_change_the_package_spec_shape() {
        let error = install_version_plan(
            ToolId::ClaudeCode,
            &probe(InstallSource::NodeManagerNpm),
            "1.2.3@attacker/pkg",
        )
        .unwrap_err();
        assert_eq!(error.message_key, "error.tool.versionInvalid");
    }

    #[test]
    fn hermes_catalog_and_exact_version_require_the_same_matching_owner() {
        let mut uv = probe(InstallSource::UvTool);
        let entry = uv.entry.as_mut().unwrap();
        entry.bin_path = PathBuf::from("/Users/a/.local/bin/hermes");
        entry.real_path = entry.bin_path.clone();
        entry.npm_package = None;
        entry.hermes_owner = Some(HermesInstallOwner::Uv {
            program_path: PathBuf::from("/Users/a/.local/bin/uv"),
        });

        let target = catalog_target(ToolId::Hermes, &uv).unwrap();
        assert_eq!(target.source, ToolInstallSource::Uv);
        assert_eq!(target.restriction, None);
        assert!(matches!(
            target.request,
            Some(VersionCatalogRequest::HermesPypi)
        ));
        let spec = &install_version_plan(ToolId::Hermes, &uv, "0.19.0")
            .unwrap()
            .attempts[0]
            .steps[0]
            .spec;
        assert_eq!(spec.program, AllowedProgram::Uv);
        assert_eq!(spec.args, vec!["tool", "install", "hermes-agent==0.19.0"]);

        uv.entry.as_mut().unwrap().hermes_owner = Some(HermesInstallOwner::Pipx {
            program_path: PathBuf::from("/Users/a/.local/bin/pipx"),
            global: false,
        });
        let blocked = catalog_target(ToolId::Hermes, &uv).unwrap();
        assert_eq!(blocked.source, ToolInstallSource::Unmanaged);
        assert_eq!(blocked.restriction, Some(ToolVersionRestriction::Unmanaged));
        assert!(blocked.request.is_none());
        assert!(install_version_plan(ToolId::Hermes, &uv, "0.19.0").is_err());
    }
}
