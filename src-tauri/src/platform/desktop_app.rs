use futures::{future::BoxFuture, StreamExt};
use serde::Deserialize;
use std::cmp::Ordering;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

use crate::domain::{
    AppError, DesktopAppId, DesktopAppInstallerHandoff, DesktopAppStatus,
    DesktopAppUninstallHandoff, ErrorCode,
};

use super::command::{AllowedProgram, CommandSpec};
use super::detached::{DetachedCommandLauncher, SystemDetachedCommandLauncher};
use super::executor::{CommandExecutor, CommandOutput, SystemExecutor};
use super::Platform;

const DESKTOP_LAUNCH_TIMEOUT: Duration = Duration::from_secs(30);
const DESKTOP_VERSION_CHECK_TIMEOUT: Duration = Duration::from_secs(6);
const MAX_PLIST_BYTES: u64 = 1024 * 1024;
const MAX_DESKTOP_VERSION_RESPONSE_BYTES: usize = 512 * 1024;

const WINDOWS_INSPECT_SCRIPT: &str = r#"$ErrorActionPreference = 'Stop'
$package = Get-AppxPackage -Name $env:AI_MANAGER_PACKAGE_NAME | Sort-Object Version -Descending | Select-Object -First 1
if ($null -eq $package) {
  [pscustomobject]@{ installed = $false; version = $null; appId = $null } | ConvertTo-Json -Compress
  exit 0
}
$start = Get-StartApps | Where-Object { $_.AppID -like ($package.PackageFamilyName + '!*') } | Select-Object -First 1
[pscustomobject]@{
  installed = $true
  version = $package.Version.ToString()
  appId = if ($null -eq $start) { $null } else { $start.AppID }
} | ConvertTo-Json -Compress"#;

const WINDOWS_LAUNCH_SCRIPT: &str = r#"$ErrorActionPreference = 'Stop'
Start-Process -FilePath 'explorer.exe' -ArgumentList ('shell:AppsFolder\' + $env:AI_MANAGER_APP_ID)"#;

const WINDOWS_UNINSTALL_SETTINGS_SCRIPT: &str = r#"$ErrorActionPreference = 'Stop'
Start-Process -FilePath 'ms-settings:appsfeatures'"#;

const OPENAI_DOWNLOAD_PAGE: &str = "https://chatgpt.com/download/";
const CLAUDE_DOWNLOAD_PAGE: &str = "https://claude.com/download";
const CURSOR_DOWNLOAD_PAGE: &str = "https://www.cursor.com/downloads";
const ZCODE_DOWNLOAD_PAGE: &str = "https://zcode.z.ai/cn";
const CHERRY_STUDIO_DOWNLOAD_PAGE: &str = "https://cherryai.com.cn/download";
const OPENAI_MACOS_APPCAST: &str = "https://persistent.oaistatic.com/codex-app-prod/appcast.xml";
const OPENAI_WINDOWS_VERSION: &str =
    "https://persistent.oaistatic.com/codex-app-prod/windows-store-update.json";
const OPENAI_MACOS_ARM64_PACKAGE: &str =
    "https://persistent.oaistatic.com/codex-app-prod/Codex.dmg";
const OPENAI_WINDOWS_X64_PACKAGE: &str =
    "https://persistent.oaistatic.com/codex-app-prod/ChatGPT-x64.msix";
const OPENAI_WINDOWS_ARM64_PACKAGE: &str =
    "https://persistent.oaistatic.com/codex-app-prod/ChatGPT-arm64.msix";
const CLAUDE_MACOS_UNIVERSAL_PACKAGE: &str =
    "https://claude.ai/api/desktop/darwin/universal/pkg/latest/redirect";
const CLAUDE_WINDOWS_X64_PACKAGE: &str =
    "https://claude.ai/api/desktop/win32/x64/msix/latest/redirect";
const CLAUDE_WINDOWS_ARM64_PACKAGE: &str =
    "https://claude.ai/api/desktop/win32/arm64/msix/latest/redirect";

/// Vendor-documented Claude Desktop MCP configuration path. OS environment
/// lookup stays inside the platform boundary; callers receive no writable path
/// on unsupported platforms.
pub fn claude_desktop_mcp_config_path() -> Option<PathBuf> {
    let isolated_home = std::env::var_os("AI_MANAGER_TEST_HOME")
        .is_some_and(|value| !value.to_string_lossy().trim().is_empty());
    let home = Some(crate::compat::ccswitch::paths::home_dir());
    // A test-home override must be authoritative on Windows too. Reusing the
    // host process's APPDATA here would escape the isolated fixture directory.
    let roaming = if isolated_home {
        None
    } else {
        std::env::var_os("APPDATA").map(PathBuf::from)
    };
    claude_desktop_mcp_config_path_for(Platform::current(), home, roaming)
}

pub(crate) fn claude_desktop_mcp_config_path_for(
    platform: Platform,
    home: Option<PathBuf>,
    roaming_app_data: Option<PathBuf>,
) -> Option<PathBuf> {
    let directory = match platform {
        Platform::MacOs => home?
            .join("Library")
            .join("Application Support")
            .join("Claude"),
        Platform::Windows => roaming_app_data
            .or_else(|| home.map(|home| home.join("AppData").join("Roaming")))?
            .join("Claude"),
        Platform::Linux | Platform::Unknown => return None,
    };
    Some(directory.join("claude_desktop_config.json"))
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DesktopAppOfficialDownloadTarget {
    pub url: &'static str,
    pub handoff: DesktopAppInstallerHandoff,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DesktopAppPlatformState {
    pub status: DesktopAppStatus,
    pub version: Option<String>,
    pub latest_version: Option<String>,
    pub can_launch: bool,
    pub environment: String,
    pub installer_handoff: DesktopAppInstallerHandoff,
    pub uninstall_handoff: DesktopAppUninstallHandoff,
    pub updates_managed_by_vendor: bool,
}

pub trait DesktopAppPlatform: Send + Sync {
    fn inspect(
        &self,
        id: DesktopAppId,
    ) -> BoxFuture<'static, Result<DesktopAppPlatformState, AppError>>;

    fn launch(&self, id: DesktopAppId) -> BoxFuture<'static, Result<(), AppError>>;

    fn open_uninstall_handoff(&self, id: DesktopAppId) -> BoxFuture<'static, Result<(), AppError>>;
}

pub struct SystemDesktopAppPlatform {
    executor: Arc<dyn CommandExecutor>,
    detached: Arc<dyn DetachedCommandLauncher>,
}

impl Default for SystemDesktopAppPlatform {
    fn default() -> Self {
        Self {
            executor: Arc::new(SystemExecutor),
            detached: Arc::new(SystemDetachedCommandLauncher),
        }
    }
}

impl DesktopAppPlatform for SystemDesktopAppPlatform {
    fn inspect(
        &self,
        id: DesktopAppId,
    ) -> BoxFuture<'static, Result<DesktopAppPlatformState, AppError>> {
        let executor = self.executor.clone();
        Box::pin(async move {
            let platform = Platform::current();
            let mut state = match platform {
                Platform::MacOs => inspect_macos(id, Path::new("/Applications"), dirs::home_dir()),
                Platform::Windows => inspect_windows(executor.as_ref(), id).await?,
                Platform::Linux => inspect_linux(id, dirs::home_dir()),
                Platform::Unknown => unsupported_state(id, Platform::Unknown),
            };
            if id == DesktopAppId::CodexApp && state.status == DesktopAppStatus::Installed {
                match fetch_official_latest_version(platform).await {
                    Ok(Some(latest)) => apply_official_latest_version(&mut state, latest),
                    Ok(None) => {}
                    Err(error) => {
                        log::debug!(
                            "[DesktopApps] Official Codex version check unavailable: {error}"
                        )
                    }
                }
            }
            Ok(state)
        })
    }

    fn launch(&self, id: DesktopAppId) -> BoxFuture<'static, Result<(), AppError>> {
        let executor = self.executor.clone();
        let detached = self.detached.clone();
        Box::pin(async move {
            match Platform::current() {
                Platform::MacOs => {
                    let bundle =
                        find_macos_bundle(id, Path::new("/Applications"), dirs::home_dir())
                            .ok_or_else(|| not_installed(id))?;
                    run_waited(
                        executor.as_ref(),
                        CommandSpec::new(
                            AllowedProgram::Open,
                            vec![bundle.to_string_lossy().into_owned()],
                        )
                        .with_program_path(PathBuf::from("/usr/bin/open"))
                        .with_sensitive_args(vec![0])
                        .with_timeout(DESKTOP_LAUNCH_TIMEOUT),
                    )
                    .await
                }
                Platform::Windows => {
                    if windows_package_name(id).is_none() {
                        return Err(launch_unsupported(id));
                    }
                    let probe = windows_probe(executor.as_ref(), id).await?;
                    if !probe.installed {
                        return Err(not_installed(id));
                    }
                    let app_id = probe
                        .app_id
                        .filter(|value| valid_windows_app_id(value))
                        .ok_or_else(|| launch_failed("registered Windows app id is unavailable"))?;
                    run_waited(executor.as_ref(), windows_launch_spec(&app_id)?).await
                }
                Platform::Linux => {
                    let program = linux_program(id).ok_or_else(|| launch_unsupported(id))?;
                    let binary =
                        find_linux_binary(id, dirs::home_dir()).ok_or_else(|| not_installed(id))?;
                    let spec = CommandSpec::new(program, Vec::new())
                        .with_program_path(binary)
                        .with_timeout(DESKTOP_LAUNCH_TIMEOUT);
                    detached
                        .launch(spec)
                        .map_err(|error| launch_failed(error.to_string()))
                }
                Platform::Unknown => Err(AppError::new(
                    ErrorCode::LaunchFailed,
                    "error.desktopApp.launchUnsupported",
                )
                .with_remediation("error.remediation.openDesktopAppManually")),
            }
        })
    }

    fn open_uninstall_handoff(&self, id: DesktopAppId) -> BoxFuture<'static, Result<(), AppError>> {
        let executor = self.executor.clone();
        Box::pin(async move {
            match Platform::current() {
                Platform::MacOs => {
                    let bundle =
                        find_macos_bundle(id, Path::new("/Applications"), dirs::home_dir())
                            .ok_or_else(|| not_installed(id))?;
                    run_uninstall_handoff(
                        executor.as_ref(),
                        CommandSpec::new(
                            AllowedProgram::Open,
                            vec!["-R".to_string(), bundle.to_string_lossy().into_owned()],
                        )
                        .with_program_path(PathBuf::from("/usr/bin/open"))
                        .with_sensitive_args(vec![1])
                        .with_timeout(DESKTOP_LAUNCH_TIMEOUT),
                    )
                    .await
                }
                Platform::Windows => {
                    if windows_package_name(id).is_none() {
                        return Err(uninstall_unsupported(id));
                    }
                    let probe = windows_probe(executor.as_ref(), id).await?;
                    if !probe.installed {
                        return Err(not_installed(id));
                    }
                    run_uninstall_handoff(executor.as_ref(), windows_uninstall_settings_spec())
                        .await
                }
                Platform::Linux | Platform::Unknown => Err(AppError::new(
                    ErrorCode::LaunchFailed,
                    "error.desktopApp.uninstallHandoffUnsupported",
                )
                .with_remediation("error.remediation.uninstallDesktopAppManually")),
            }
        })
    }
}

fn unsupported_state(id: DesktopAppId, platform: Platform) -> DesktopAppPlatformState {
    let download = official_download_target(id, platform, std::env::consts::ARCH);
    DesktopAppPlatformState {
        status: DesktopAppStatus::Unsupported,
        version: None,
        latest_version: None,
        can_launch: false,
        environment: platform.as_str().to_string(),
        installer_handoff: download.handoff,
        uninstall_handoff: DesktopAppUninstallHandoff::Unsupported,
        updates_managed_by_vendor: platform != Platform::Unknown,
    }
}

fn inspect_macos(
    id: DesktopAppId,
    system_applications: &Path,
    home: Option<PathBuf>,
) -> DesktopAppPlatformState {
    let bundle = find_macos_bundle(id, system_applications, home);
    DesktopAppPlatformState {
        status: if bundle.is_some() {
            DesktopAppStatus::Installed
        } else {
            DesktopAppStatus::NotInstalled
        },
        version: bundle.as_deref().and_then(read_bundle_version),
        latest_version: None,
        can_launch: bundle.is_some(),
        environment: Platform::MacOs.as_str().to_string(),
        installer_handoff: official_download_target(id, Platform::MacOs, std::env::consts::ARCH)
            .handoff,
        uninstall_handoff: DesktopAppUninstallHandoff::RevealApplication,
        updates_managed_by_vendor: true,
    }
}

fn find_macos_bundle(
    id: DesktopAppId,
    system_applications: &Path,
    home: Option<PathBuf>,
) -> Option<PathBuf> {
    let user_applications = home.map(|home| home.join("Applications"));
    macos_identities(id).iter().find_map(|identity| {
        let system_candidate = system_applications.join(identity.bundle_name);
        let user_candidate = user_applications
            .as_ref()
            .map(|applications| applications.join(identity.bundle_name));
        [Some(system_candidate), user_candidate]
            .into_iter()
            .flatten()
            .find(|candidate| {
                candidate.is_dir()
                    && candidate
                        .join("Contents")
                        .join("MacOS")
                        .join(identity.executable_name)
                        .is_file()
                    && identity_matches_bundle(candidate, identity)
            })
    })
}

struct MacOsIdentity {
    bundle_name: &'static str,
    executable_name: &'static str,
    bundle_identifiers: &'static [&'static str],
}

const OPENAI_MACOS_IDENTITIES: &[MacOsIdentity] = &[
    MacOsIdentity {
        bundle_name: "ChatGPT.app",
        executable_name: "ChatGPT",
        bundle_identifiers: &[],
    },
    MacOsIdentity {
        bundle_name: "Codex.app",
        executable_name: "Codex",
        bundle_identifiers: &[],
    },
];
const CLAUDE_MACOS_IDENTITIES: &[MacOsIdentity] = &[MacOsIdentity {
    bundle_name: "Claude.app",
    executable_name: "Claude",
    bundle_identifiers: &[],
}];
const CURSOR_MACOS_IDENTITIES: &[MacOsIdentity] = &[MacOsIdentity {
    bundle_name: "Cursor.app",
    executable_name: "Cursor",
    bundle_identifiers: &["com.todesktop.230313mzl4w4u92"],
}];
const ZCODE_MACOS_IDENTITIES: &[MacOsIdentity] = &[MacOsIdentity {
    bundle_name: "ZCode.app",
    executable_name: "ZCode",
    bundle_identifiers: &["dev.zcode.app"],
}];
const CHERRY_STUDIO_MACOS_IDENTITIES: &[MacOsIdentity] = &[MacOsIdentity {
    bundle_name: "Cherry Studio.app",
    executable_name: "Cherry Studio",
    bundle_identifiers: &["com.kangfenmao.CherryStudio"],
}];

fn macos_identities(id: DesktopAppId) -> &'static [MacOsIdentity] {
    match id {
        DesktopAppId::CodexApp => OPENAI_MACOS_IDENTITIES,
        DesktopAppId::ClaudeDesktop => CLAUDE_MACOS_IDENTITIES,
        DesktopAppId::Cursor => CURSOR_MACOS_IDENTITIES,
        DesktopAppId::ZCode => ZCODE_MACOS_IDENTITIES,
        DesktopAppId::CherryStudio => CHERRY_STUDIO_MACOS_IDENTITIES,
    }
}

fn identity_matches_bundle(bundle: &Path, identity: &MacOsIdentity) -> bool {
    identity.bundle_identifiers.is_empty()
        || read_plist_string(bundle, "CFBundleIdentifier").is_some_and(|identifier| {
            identity
                .bundle_identifiers
                .iter()
                .any(|expected| identifier == *expected)
        })
}

fn read_bundle_version(bundle: &Path) -> Option<String> {
    read_plist_string(bundle, "CFBundleShortVersionString")
        .filter(|value| valid_version(value))
        .or_else(|| {
            read_plist_string(bundle, "CFBundleVersion").filter(|value| valid_version(value))
        })
}

fn read_plist_string(bundle: &Path, key: &str) -> Option<String> {
    let plist = bundle.join("Contents").join("Info.plist");
    if std::fs::metadata(&plist).ok()?.len() > MAX_PLIST_BYTES {
        return None;
    }
    let source = std::fs::read_to_string(plist).ok()?;
    plist_string(&source, key)
}

fn plist_string(source: &str, key: &str) -> Option<String> {
    let marker = format!("<key>{key}</key>");
    let after_key = source.split_once(&marker)?.1;
    let after_open = after_key.split_once("<string>")?.1;
    let value = after_open.split_once("</string>")?.0.trim();
    (!value.is_empty()
        && value.len() <= 256
        && value
            .chars()
            .all(|character| !character.is_control() && !matches!(character, '<' | '>' | '&')))
    .then(|| value.to_string())
}

fn inspect_linux(id: DesktopAppId, home: Option<PathBuf>) -> DesktopAppPlatformState {
    if linux_program(id).is_none() {
        return unsupported_state(id, Platform::Linux);
    }
    let binary = find_linux_binary(id, home);
    DesktopAppPlatformState {
        status: if binary.is_some() {
            DesktopAppStatus::Installed
        } else {
            DesktopAppStatus::NotInstalled
        },
        version: None,
        latest_version: None,
        can_launch: binary.is_some(),
        environment: Platform::Linux.as_str().to_string(),
        installer_handoff: DesktopAppInstallerHandoff::OfficialDownloadPage,
        uninstall_handoff: DesktopAppUninstallHandoff::Unsupported,
        updates_managed_by_vendor: true,
    }
}

fn find_linux_binary(id: DesktopAppId, home: Option<PathBuf>) -> Option<PathBuf> {
    let binary = linux_program(id)?.as_str();
    let mut candidates = vec![
        PathBuf::from("/usr/bin").join(binary),
        PathBuf::from("/usr/local/bin").join(binary),
    ];
    if let Some(home) = home {
        candidates.push(home.join(".local").join("bin").join(binary));
    }
    candidates.into_iter().find(|candidate| candidate.is_file())
}

fn linux_program(id: DesktopAppId) -> Option<AllowedProgram> {
    match id {
        DesktopAppId::CodexApp => Some(AllowedProgram::ChatGptDesktop),
        DesktopAppId::ClaudeDesktop => Some(AllowedProgram::ClaudeDesktop),
        DesktopAppId::Cursor | DesktopAppId::ZCode | DesktopAppId::CherryStudio => None,
    }
}

async fn inspect_windows(
    executor: &dyn CommandExecutor,
    id: DesktopAppId,
) -> Result<DesktopAppPlatformState, AppError> {
    if windows_package_name(id).is_none() {
        return Ok(unsupported_state(id, Platform::Windows));
    }
    let probe = windows_probe(executor, id).await?;
    Ok(DesktopAppPlatformState {
        status: if probe.installed {
            DesktopAppStatus::Installed
        } else {
            DesktopAppStatus::NotInstalled
        },
        version: probe.version.filter(|value| valid_version(value)),
        latest_version: None,
        can_launch: probe.app_id.as_deref().is_some_and(valid_windows_app_id),
        environment: Platform::Windows.as_str().to_string(),
        installer_handoff: official_download_target(id, Platform::Windows, std::env::consts::ARCH)
            .handoff,
        uninstall_handoff: DesktopAppUninstallHandoff::SystemSettings,
        updates_managed_by_vendor: true,
    })
}

pub fn current_official_download_target(id: DesktopAppId) -> DesktopAppOfficialDownloadTarget {
    official_download_target(id, Platform::current(), std::env::consts::ARCH)
}

fn official_download_target(
    id: DesktopAppId,
    platform: Platform,
    architecture: &str,
) -> DesktopAppOfficialDownloadTarget {
    let direct_url = match (id, platform, architecture) {
        (DesktopAppId::CodexApp, Platform::MacOs, "aarch64") => Some(OPENAI_MACOS_ARM64_PACKAGE),
        (DesktopAppId::CodexApp, Platform::Windows, "x86_64") => Some(OPENAI_WINDOWS_X64_PACKAGE),
        (DesktopAppId::CodexApp, Platform::Windows, "aarch64") => {
            Some(OPENAI_WINDOWS_ARM64_PACKAGE)
        }
        (DesktopAppId::ClaudeDesktop, Platform::MacOs, "x86_64" | "aarch64") => {
            Some(CLAUDE_MACOS_UNIVERSAL_PACKAGE)
        }
        (DesktopAppId::ClaudeDesktop, Platform::Windows, "x86_64") => {
            Some(CLAUDE_WINDOWS_X64_PACKAGE)
        }
        (DesktopAppId::ClaudeDesktop, Platform::Windows, "aarch64") => {
            Some(CLAUDE_WINDOWS_ARM64_PACKAGE)
        }
        _ => None,
    };
    match direct_url {
        Some(url) => DesktopAppOfficialDownloadTarget {
            url,
            handoff: DesktopAppInstallerHandoff::DirectOfficialPackage,
        },
        None => DesktopAppOfficialDownloadTarget {
            url: match id {
                DesktopAppId::CodexApp => OPENAI_DOWNLOAD_PAGE,
                DesktopAppId::ClaudeDesktop => CLAUDE_DOWNLOAD_PAGE,
                DesktopAppId::Cursor => CURSOR_DOWNLOAD_PAGE,
                DesktopAppId::ZCode => ZCODE_DOWNLOAD_PAGE,
                DesktopAppId::CherryStudio => CHERRY_STUDIO_DOWNLOAD_PAGE,
            },
            handoff: if platform == Platform::Unknown {
                DesktopAppInstallerHandoff::Unsupported
            } else {
                DesktopAppInstallerHandoff::OfficialDownloadPage
            },
        },
    }
}

async fn fetch_official_latest_version(platform: Platform) -> Result<Option<String>, String> {
    match platform {
        Platform::MacOs => {
            let source =
                fetch_bounded_official_text(OPENAI_MACOS_APPCAST, "application/xml").await?;
            parse_codex_macos_appcast(&source)
                .ok_or_else(|| "official macOS appcast did not contain a valid version".to_string())
                .map(Some)
        }
        Platform::Windows => {
            let source =
                fetch_bounded_official_text(OPENAI_WINDOWS_VERSION, "application/json").await?;
            parse_codex_windows_version(&source)
                .ok_or_else(|| {
                    "official Windows version response did not contain a valid version".to_string()
                })
                .map(Some)
        }
        Platform::Linux | Platform::Unknown => Ok(None),
    }
}

async fn fetch_bounded_official_text(
    url: &'static str,
    accept: &'static str,
) -> Result<String, String> {
    tokio::time::timeout(DESKTOP_VERSION_CHECK_TIMEOUT, async move {
        let response = crate::compat::ccswitch::network_proxy::http_client_for_fixed_url(url)
            .get(url)
            .header(reqwest::header::ACCEPT, accept)
            .send()
            .await
            .map_err(|error| error.to_string())?;
        if !response.status().is_success() {
            return Err(format!(
                "official version endpoint returned {}",
                response.status()
            ));
        }
        if response
            .content_length()
            .is_some_and(|length| length > MAX_DESKTOP_VERSION_RESPONSE_BYTES as u64)
        {
            return Err("official version response exceeded the size limit".to_string());
        }

        let mut body = Vec::new();
        let mut stream = response.bytes_stream();
        while let Some(chunk) = stream.next().await {
            let chunk = chunk.map_err(|error| error.to_string())?;
            if body.len().saturating_add(chunk.len()) > MAX_DESKTOP_VERSION_RESPONSE_BYTES {
                return Err("official version response exceeded the size limit".to_string());
            }
            body.extend_from_slice(&chunk);
        }
        String::from_utf8(body).map_err(|_| "official version response was not UTF-8".to_string())
    })
    .await
    .map_err(|_| "official version check timed out".to_string())?
}

fn parse_codex_macos_appcast(source: &str) -> Option<String> {
    const OPEN: &str = "<sparkle:shortVersionString>";
    const CLOSE: &str = "</sparkle:shortVersionString>";

    let mut rest = source;
    let mut latest: Option<String> = None;
    for _ in 0..128 {
        let Some((_, after_open)) = rest.split_once(OPEN) else {
            break;
        };
        let Some((raw, after_close)) = after_open.split_once(CLOSE) else {
            break;
        };
        let candidate = raw.trim();
        if parse_numeric_version(candidate).is_some()
            && latest.as_deref().is_none_or(|current| {
                compare_desktop_versions(candidate, current) == Some(Ordering::Greater)
            })
        {
            latest = Some(candidate.to_string());
        }
        rest = after_close;
    }
    latest
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct CodexWindowsVersion {
    schema_version: u8,
    build_version: String,
    package_identity: String,
}

fn parse_codex_windows_version(source: &str) -> Option<String> {
    let response: CodexWindowsVersion = serde_json::from_str(source).ok()?;
    (response.schema_version == 1
        && response.package_identity == "OpenAI.Codex"
        && parse_numeric_version(&response.build_version).is_some())
    .then_some(response.build_version)
}

fn apply_official_latest_version(state: &mut DesktopAppPlatformState, latest: String) {
    if parse_numeric_version(&latest).is_none() {
        return;
    }
    let has_update = state.version.as_deref().is_some_and(|current| {
        compare_desktop_versions(&latest, current) == Some(Ordering::Greater)
    });
    state.latest_version = Some(latest);
    if has_update {
        state.status = DesktopAppStatus::UpdateAvailable;
    }
}

fn compare_desktop_versions(left: &str, right: &str) -> Option<Ordering> {
    Some(parse_numeric_version(left)?.cmp(&parse_numeric_version(right)?))
}

fn parse_numeric_version(value: &str) -> Option<[u64; 4]> {
    let mut parsed = [0; 4];
    let parts = value.split('.').collect::<Vec<_>>();
    if !(2..=4).contains(&parts.len()) {
        return None;
    }
    for (index, part) in parts.into_iter().enumerate() {
        if part.is_empty() || part.len() > 20 || !part.bytes().all(|byte| byte.is_ascii_digit()) {
            return None;
        }
        parsed[index] = part.parse().ok()?;
    }
    Some(parsed)
}

#[derive(Debug, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
struct WindowsProbe {
    installed: bool,
    version: Option<String>,
    app_id: Option<String>,
}

async fn windows_probe(
    executor: &dyn CommandExecutor,
    id: DesktopAppId,
) -> Result<WindowsProbe, AppError> {
    let spec = windows_inspect_spec(id).ok_or_else(|| {
        inventory_failed(format!(
            "{} has no audited Windows package identity",
            id.as_str()
        ))
    })?;
    let output = executor
        .execute(spec)
        .await
        .map_err(|error| inventory_failed(error.to_string()))?;
    if !output.success {
        return Err(inventory_failed(output.technical_detail()));
    }
    parse_windows_probe(&output.stdout)
        .ok_or_else(|| inventory_failed("Windows desktop inventory returned invalid data"))
}

fn windows_inspect_spec(id: DesktopAppId) -> Option<CommandSpec> {
    Some(
        CommandSpec::powershell_script(WINDOWS_INSPECT_SCRIPT)
            .with_env("AI_MANAGER_PACKAGE_NAME", windows_package_name(id)?)
            .with_sensitive_args(vec![4])
            .with_timeout(DESKTOP_LAUNCH_TIMEOUT),
    )
}

fn windows_launch_spec(app_id: &str) -> Result<CommandSpec, AppError> {
    if !valid_windows_app_id(app_id) {
        return Err(launch_failed("registered Windows app id is invalid"));
    }
    Ok(CommandSpec::powershell_script(WINDOWS_LAUNCH_SCRIPT)
        .with_env("AI_MANAGER_APP_ID", app_id)
        .with_sensitive_args(vec![4])
        .with_timeout(DESKTOP_LAUNCH_TIMEOUT))
}

fn windows_uninstall_settings_spec() -> CommandSpec {
    CommandSpec::powershell_script(WINDOWS_UNINSTALL_SETTINGS_SCRIPT)
        .with_sensitive_args(vec![4])
        .with_timeout(DESKTOP_LAUNCH_TIMEOUT)
}

fn windows_package_name(id: DesktopAppId) -> Option<&'static str> {
    match id {
        DesktopAppId::CodexApp => Some("OpenAI.Codex"),
        DesktopAppId::ClaudeDesktop => Some("Claude"),
        DesktopAppId::Cursor | DesktopAppId::ZCode | DesktopAppId::CherryStudio => None,
    }
}

fn parse_windows_probe(output: &str) -> Option<WindowsProbe> {
    let json = output
        .lines()
        .rev()
        .map(str::trim)
        .find(|line| line.starts_with('{') && line.ends_with('}'))?
        .trim_start_matches('\u{feff}');
    serde_json::from_str(json).ok()
}

fn valid_windows_app_id(value: &str) -> bool {
    value.len() <= 256
        && value.contains('!')
        && value.chars().all(|character| {
            character.is_ascii_alphanumeric() || matches!(character, '.' | '_' | '-' | '!')
        })
}

fn valid_version(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 128
        && value.chars().all(|character| {
            !character.is_control() && character != '<' && character != '>' && character != '&'
        })
}

async fn run_waited(executor: &dyn CommandExecutor, spec: CommandSpec) -> Result<(), AppError> {
    let output: CommandOutput = executor
        .execute(spec)
        .await
        .map_err(|error| launch_failed(error.to_string()))?;
    if output.success {
        Ok(())
    } else {
        Err(launch_failed(output.technical_detail()))
    }
}

async fn run_uninstall_handoff(
    executor: &dyn CommandExecutor,
    spec: CommandSpec,
) -> Result<(), AppError> {
    let output = executor
        .execute(spec)
        .await
        .map_err(|error| uninstall_handoff_failed(error.to_string()))?;
    if output.success {
        Ok(())
    } else {
        Err(uninstall_handoff_failed(output.technical_detail()))
    }
}

fn not_installed(id: DesktopAppId) -> AppError {
    AppError::new(ErrorCode::ToolNotFound, "error.desktopApp.notFound")
        .with_technical(id.as_str())
        .with_remediation("error.remediation.installManually")
}

fn inventory_failed(technical: impl Into<String>) -> AppError {
    AppError::new(ErrorCode::Internal, "error.desktopApp.inventoryFailed")
        .with_technical(technical)
        .with_remediation("error.remediation.retryOrViewDetails")
}

fn launch_failed(technical: impl Into<String>) -> AppError {
    AppError::new(ErrorCode::LaunchFailed, "error.desktopApp.launchFailed")
        .with_technical(technical)
        .with_remediation("error.remediation.openDesktopAppManually")
}

fn launch_unsupported(id: DesktopAppId) -> AppError {
    AppError::new(
        ErrorCode::LaunchFailed,
        "error.desktopApp.launchUnsupported",
    )
    .with_technical(format!(
        "{} has no audited launcher identity on this platform",
        id.as_str()
    ))
    .with_remediation("error.remediation.openDesktopAppManually")
}

fn uninstall_unsupported(id: DesktopAppId) -> AppError {
    AppError::new(
        ErrorCode::LaunchFailed,
        "error.desktopApp.uninstallHandoffUnsupported",
    )
    .with_technical(format!(
        "{} has no audited uninstall identity on this platform",
        id.as_str()
    ))
    .with_remediation("error.remediation.uninstallDesktopAppManually")
}

fn uninstall_handoff_failed(technical: impl Into<String>) -> AppError {
    AppError::new(
        ErrorCode::LaunchFailed,
        "error.desktopApp.uninstallHandoffFailed",
    )
    .with_technical(technical)
    .with_remediation("error.remediation.uninstallDesktopAppManually")
}

#[cfg(test)]
#[path = "desktop_app/tests.rs"]
mod tests;
