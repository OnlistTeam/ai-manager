#![allow(non_snake_case)]

use crate::init_status::InitErrorPayload;
use once_cell::sync::Lazy;
use regex::Regex;
use std::path::Path;

#[cfg(target_os = "windows")]
use std::os::windows::process::CommandExt;

#[cfg(target_os = "windows")]
const CREATE_NO_WINDOW: u32 = 0x08000000;

/// Get the initialization error from app startup, if any.
/// The frontend pulls this early to avoid missing the notice due to an event subscription race.
#[tauri::command]
pub async fn get_init_error() -> Result<Option<InitErrorPayload>, String> {
    Ok(crate::init_status::get_init_error())
}

#[derive(serde::Serialize)]
pub struct ToolVersion {
    pub(crate) name: String,
    pub(crate) version: Option<String>,
    pub(crate) latest_version: Option<String>, // latest available version
    pub(crate) error: Option<String>,
    /// The executable was located but `--version` exited with an error (installed yet unusable, e.g. the Node version is too old).
    /// Lets the frontend tell "not installed" from "installed but broken" without inferring semantics from the error text.
    pub(crate) installed_but_broken: bool,
    /// Tool runtime environment: "windows", "wsl", "macos", "linux", "unknown"
    pub(crate) env_type: String,
    /// When env_type is "wsl", the WSL distro this tool is bound to (used to probe shells per distro)
    pub(crate) wsl_distro: Option<String>,
}

const VALID_TOOLS: [&str; 10] = [
    "claude", "codex", "gemini", "grok", "opencode", "openclaw", "hermes", "pi", "kimi", "dsh",
];

// Keep platform-specific env detection in one place to avoid repeating cfg blocks.
#[cfg(target_os = "windows")]
fn tool_env_type_and_wsl_distro(tool: &str) -> (String, Option<String>) {
    if let Some(distro) = wsl_distro_for_tool(tool) {
        ("wsl".to_string(), Some(distro))
    } else {
        ("windows".to_string(), None)
    }
}

#[cfg(target_os = "macos")]
fn tool_env_type_and_wsl_distro(_tool: &str) -> (String, Option<String>) {
    ("macos".to_string(), None)
}

#[cfg(target_os = "linux")]
fn tool_env_type_and_wsl_distro(_tool: &str) -> (String, Option<String>) {
    ("linux".to_string(), None)
}

#[cfg(not(any(target_os = "windows", target_os = "macos", target_os = "linux")))]
fn tool_env_type_and_wsl_distro(_tool: &str) -> (String, Option<String>) {
    ("unknown".to_string(), None)
}

/// Take at most the last `n` lines of the text (the key npm / pip errors usually appear at the end).
fn last_lines(text: &str, n: usize) -> String {
    let lines: Vec<&str> = text.lines().collect();
    let start = lines.len().saturating_sub(n);
    lines[start..].join("\n")
}

pub(crate) fn decode_command_output(bytes: &[u8]) -> String {
    #[cfg(target_os = "windows")]
    {
        decode_windows_command_output(bytes)
    }

    #[cfg(not(target_os = "windows"))]
    {
        String::from_utf8_lossy(bytes).into_owned()
    }
}

#[cfg(target_os = "windows")]
fn decode_windows_command_output(bytes: &[u8]) -> String {
    if bytes.is_empty() {
        return String::new();
    }

    if let Ok(text) = std::str::from_utf8(bytes) {
        return text.to_string();
    }

    use windows_sys::Win32::Globalization::{GetACP, GetOEMCP, MultiByteToWideChar};

    fn decode_codepage(bytes: &[u8], codepage: u32) -> Option<String> {
        if codepage == 0 {
            return None;
        }

        let input_len = i32::try_from(bytes.len()).ok()?;
        unsafe {
            let wide_len = MultiByteToWideChar(
                codepage,
                0,
                bytes.as_ptr(),
                input_len,
                std::ptr::null_mut(),
                0,
            );
            if wide_len <= 0 {
                return None;
            }

            let mut wide = vec![0u16; wide_len as usize];
            let written = MultiByteToWideChar(
                codepage,
                0,
                bytes.as_ptr(),
                input_len,
                wide.as_mut_ptr(),
                wide_len,
            );
            if written <= 0 {
                return None;
            }

            Some(String::from_utf16_lossy(&wide[..written as usize]))
        }
    }

    let oem_cp = unsafe { GetOEMCP() };
    if let Some(decoded) = decode_codepage(bytes, oem_cp) {
        return decoded;
    }

    let ansi_cp = unsafe { GetACP() };
    if ansi_cp != oem_cp {
        if let Some(decoded) = decode_codepage(bytes, ansi_cp) {
            return decoded;
        }
    }

    String::from_utf8_lossy(bytes).into_owned()
}

#[derive(Debug, Clone, Copy)]
pub(crate) enum LifecycleCommandShell {
    #[cfg_attr(target_os = "windows", allow(dead_code))]
    Posix,
    #[cfg_attr(not(target_os = "windows"), allow(dead_code))]
    WindowsBatch,
}

pub(crate) fn official_update_args(tool: &str) -> Option<&'static str> {
    match tool {
        "claude" | "codex" | "grok" | "hermes" => Some("update"),
        "kimi" => Some("upgrade"),
        "openclaw" => Some("update --yes"),
        "opencode" => Some("upgrade"),
        _ => None,
    }
}

/// Base primitive for Windows double quoting: always quote and escape inner `"` as `\"`.
/// Both `windows_cmd_double_quote_arg` (for bash command strings passed to wsl.exe) and
/// `win_quote_path_for_batch` (for anchored paths) build on it, so two quoters cannot drift apart
/// and produce inconsistent quoting for the same path. Mirrors the two-layer "heavy base +
/// lightweight conditional wrapper" structure of the POSIX `shell_single_quote` and `quote_path_if_spaced`.
#[cfg(target_os = "windows")]
fn win_double_quote(value: &str) -> String {
    format!("\"{}\"", value.replace('"', "\\\""))
}

#[cfg(target_os = "windows")]
fn windows_cmd_double_quote_arg(value: &str) -> String {
    win_double_quote(value)
}

/// Local probing only: determine the runtime environment and probe the CLI version without a single
/// network request; `latest_version` is always `None`.
///
/// The product split "local inventory" and "look up the latest version online" into two commands (the
/// former opens instantly, the latter fills in version numbers in the background). Both take the local
/// version from here, so there is only one platform branch and the two cannot drift.
pub(crate) fn probe_local_tool_version(
    tool: &str,
    wsl_shell: Option<&str>,
    wsl_shell_flag: Option<&str>,
) -> ToolVersion {
    debug_assert!(
        VALID_TOOLS.contains(&tool),
        "unexpected tool name in probe_local_tool_version: {tool}"
    );

    // Determine this tool's runtime environment and WSL distro (if any)
    let (env_type, wsl_distro) = tool_env_type_and_wsl_distro(tool);

    let probe = if let Some(distro) = wsl_distro.as_deref() {
        try_get_version_wsl(tool, distro, wsl_shell, wsl_shell_flag)
    } else {
        #[cfg(target_os = "windows")]
        {
            // Probe the PATH-default entry (what `tool` resolves to in a
            // terminal) first, and only fall back to the directory scan when it
            // is genuinely absent (NotFound). Two goals:
            // 1. Keep the displayed "current version" aligned with the version
            //    the user actually runs — a stale shim in a hardcoded fallback
            //    dir (e.g. an old `%APPDATA%\npm`) must not override a newer
            //    PATH install (#4701: "updated but still shows the old version").
            // 2. Mirror the non-Windows structure (`try_get_version` →
            //    `scan_cli_version`).
            // `probe_path_default_version` executes only the real executable
            //    resolved by `where` (App Execution Aliases filtered out), so
            //    it never `cmd /C tool` into a protocol handler.
            match probe_path_default_version(tool) {
                ShellProbe::NotFound(_) => scan_cli_version(tool),
                found => found,
            }
        }

        #[cfg(not(target_os = "windows"))]
        {
            // The first command on PATH wins; only if it is really absent (NotFound) do we scan common directories.
            match try_get_version(tool) {
                ShellProbe::NotFound(_) => scan_cli_version(tool),
                found => found,
            }
        }
    };
    let (local_version, local_error, installed_but_broken) = match probe {
        ShellProbe::Found(v) => (Some(v), None, false),
        ShellProbe::FoundButFailed(e) => (None, Some(e), true),
        ShellProbe::NotFound(e) => (None, Some(e), false),
    };

    ToolVersion {
        name: tool.to_string(),
        version: local_version,
        latest_version: None,
        error: local_error,
        installed_but_broken,
        env_type,
        wsl_distro,
    }
}

/// Get the version info of a single tool (internal implementation): local probing plus an online latest-version lookup.
pub(crate) async fn get_single_tool_version_impl(
    tool: &str,
    wsl_shell: Option<&str>,
    wsl_shell_flag: Option<&str>,
) -> ToolVersion {
    let mut probed = probe_local_tool_version(tool, wsl_shell, wsl_shell_flag);

    // Use the global HTTP client (proxy configuration included)
    let client = crate::proxy::http_client::get();

    // Remote latest version (npm tools additionally query the prerelease channel when the local version leads latest; see
    // fetch_npm_latest_for_tool / npm_prerelease_tags）
    let local = probed.version.as_deref();
    let latest_version = match tool {
        "claude" => {
            fetch_npm_latest_for_tool(&client, "@anthropic-ai/claude-code", tool, local).await
        }
        "codex" => fetch_npm_latest_for_tool(&client, "@openai/codex", tool, local).await,
        "gemini" => fetch_npm_latest_for_tool(&client, "@google/gemini-cli", tool, local).await,
        "grok" => fetch_npm_latest_for_tool(&client, "@xai-official/grok", tool, local).await,
        "opencode" => {
            if let Some(version) =
                fetch_npm_latest_for_tool(&client, "opencode-ai", tool, local).await
            {
                Some(version)
            } else {
                fetch_github_latest_version(&client, "anomalyco/opencode").await
            }
        }
        "openclaw" => fetch_npm_latest_for_tool(&client, "openclaw", tool, local).await,
        "hermes" => fetch_hermes_latest_version(&client, local).await,
        "pi" => {
            fetch_npm_latest_for_tool(&client, "@earendil-works/pi-coding-agent", tool, local).await
        }
        "kimi" => fetch_npm_latest_for_tool(&client, "@moonshot-ai/kimi-code", tool, local).await,
        "dsh" => fetch_npm_latest_for_tool(&client, "@deepseek-ai/dsh", tool, local).await,
        _ => None,
    };
    probed.latest_version = latest_version;
    probed
}

/// Prerelease channel tags of this tool on npm (earlier entries win). Only queried when the local
/// version **strictly leads** `latest` — so users who deliberately follow an early channel (e.g. Claude
/// Code's `next`) see the latest version of their channel, while stable-channel users are never exposed
/// to a prerelease. An empty slice means the tool only looks at `latest`.
///
/// Why not cover every tool: prerelease tag naming differs per vendor (codex=alpha/beta/native,
/// gemini=nightly/preview, openclaw=alpha/beta), codex beta/native use timestamp-style `0.1.x` versions
/// and gemini has a mispublished `false` tag. Those dirty values would be filtered out by the version
/// comparison in `pick_latest_version`, but the maintenance cost and false-positive risk are not worth it,
/// so this is enabled for Claude Code only.
fn npm_prerelease_tags(tool: &str) -> &'static [&'static str] {
    match tool {
        "claude" => &["next"],
        _ => &[],
    }
}

/// Parse "2.1.156" / "2.1.156-beta.1" into (three core segments, prerelease segments). Returns None when unparseable.
/// Symmetric with the parseVersion semantics of the frontend `src/lib/version.ts` (one implementation per language).
/// patch uses u64 so codex's timestamp-style `0.1.2505172116` does not overflow.
fn parse_semver(v: &str) -> Option<([u64; 3], Vec<String>)> {
    // Ignore `+build` metadata, then split off the prerelease part at the first `-`.
    let core_and_pre = v.trim().split('+').next().unwrap_or("");
    let (core, pre) = match core_and_pre.split_once('-') {
        Some((c, p)) => (c, Some(p)),
        None => (core_and_pre, None),
    };
    let mut parts = core.split('.');
    let major = parts.next()?.parse::<u64>().ok()?;
    let minor = parts.next()?.parse::<u64>().ok()?;
    let patch = parts.next()?.parse::<u64>().ok()?;
    if parts.next().is_some() {
        return None; // more than three segments, invalid
    }
    let pre_segments = pre
        .map(|p| p.split('.').map(|s| s.to_string()).collect())
        .unwrap_or_default();
    Some(([major, minor, patch], pre_segments))
}

/// Compare two versions (semver rules: the three core segments come first; with equal cores a prerelease is
/// lower than a release; prerelease segments compare one by one — numeric segments numerically, numeric <
/// non-numeric, non-numeric by ASCII, and with equal prefixes more segments wins). Returns None when either
/// side is unparseable, so callers can act conservatively.
pub(crate) fn compare_semver(a: &str, b: &str) -> Option<std::cmp::Ordering> {
    use std::cmp::Ordering;
    let (ac, ap) = parse_semver(a)?;
    let (bc, bp) = parse_semver(b)?;
    for i in 0..3 {
        match ac[i].cmp(&bc[i]) {
            Ordering::Equal => continue,
            other => return Some(other),
        }
    }
    match (ap.is_empty(), bp.is_empty()) {
        (true, true) => return Some(Ordering::Equal),
        (true, false) => return Some(Ordering::Greater),
        (false, true) => return Some(Ordering::Less),
        (false, false) => {}
    }
    for (x, y) in ap.iter().zip(bp.iter()) {
        let ord = match (x.parse::<u64>(), y.parse::<u64>()) {
            (Ok(xv), Ok(yv)) => xv.cmp(&yv),
            (Ok(_), Err(_)) => Ordering::Less, // numeric segment < non-numeric segment
            (Err(_), Ok(_)) => Ordering::Greater,
            (Err(_), Err(_)) => x.as_str().cmp(y.as_str()),
        };
        if ord != Ordering::Equal {
            return Some(ord);
        }
    }
    Some(ap.len().cmp(&bp.len()))
}

/// Pick the "latest version" to display from the full dist-tags of one registry request.
///
/// Rule: `latest` by default; only when the local version **strictly leads** `latest` (meaning the user
/// deliberately follows an early channel) are the versions behind `prerelease_tags` considered, taking the
/// highest one that parses and exceeds `latest`. Dirty tags that do not parse or do not exceed latest lose.
fn pick_latest_version(
    dist_tags: &serde_json::Map<String, serde_json::Value>,
    prerelease_tags: &[&str],
    local_version: Option<&str>,
    published_versions: Option<&serde_json::Map<String, serde_json::Value>>,
) -> Option<String> {
    use std::cmp::Ordering;
    let latest = dist_tags.get("latest").and_then(|v| v.as_str())?;

    // Grok has previously published newer stable builds while its `latest`
    // tag still pointed at an old 0.x release. A software manager should not
    // make users discover that registry mistake in a manual version picker.
    let mut best = published_versions
        .and_then(|versions| highest_stable_version(versions.keys().map(String::as_str)))
        .filter(|version| compare_semver(version, latest) == Some(Ordering::Greater))
        .unwrap_or_else(|| latest.to_string());

    // Does the local version strictly lead latest? If either side is unparseable, assume it does not (latest only).
    let local_ahead = local_version
        .and_then(|local| compare_semver(local, &best))
        .map(|ord| ord == Ordering::Greater)
        .unwrap_or(false);
    if prerelease_tags.is_empty() || !local_ahead {
        return Some(best);
    }

    for tag in prerelease_tags {
        if let Some(candidate) = dist_tags.get(*tag).and_then(|v| v.as_str()) {
            if compare_semver(candidate, &best) == Some(Ordering::Greater) {
                best = candidate.to_string();
            }
        }
    }
    Some(best)
}

pub(crate) fn highest_stable_version<'a>(
    versions: impl Iterator<Item = &'a str>,
) -> Option<String> {
    versions
        .filter(|version| {
            parse_semver(version)
                .map(|(_, prerelease)| prerelease.is_empty())
                .unwrap_or(false)
        })
        .max_by(|left, right| compare_semver(left, right).unwrap_or_else(|| left.cmp(right)))
        .map(str::to_string)
}

/// Registry endpoint that returns dist-tags only. Full metadata downloads tens of megabytes just for one
/// `latest` string (`@openai/codex` measured at 13 MB); this is the 600 B version of the same tag table.
/// The `/` in a scoped name is part of the path and the registry accepts it directly, no URL encoding needed.
fn npm_dist_tags_url(package: &str) -> String {
    format!("https://registry.npmjs.org/-/package/{package}/dist-tags")
}

/// Fetch the dist-tags table of an npm package (tag -> version), including prerelease channels such as `next` / `alpha`.
async fn fetch_npm_dist_tags(
    client: &reqwest::Client,
    package: &str,
) -> Option<serde_json::Map<String, serde_json::Value>> {
    let resp = client
        .get(npm_dist_tags_url(package))
        .timeout(LATEST_PROBE_TIMEOUT)
        .send()
        .await
        .ok()?;
    match resp.json::<serde_json::Value>().await.ok()? {
        serde_json::Value::Object(tags) => Some(tags),
        _ => None,
    }
}

/// Fetch the full metadata of an npm package (the complete manifest of every published version, tens of MB).
/// Only grok needs it — `pick_latest_version` has to scan `versions` to correct its wrong registry tag;
/// everything the other tools need is already in the dist-tags endpoint.
async fn fetch_npm_metadata(client: &reqwest::Client, package: &str) -> Option<serde_json::Value> {
    let url = format!("https://registry.npmjs.org/{package}");
    let resp = client
        .get(&url)
        .timeout(LATEST_PROBE_TIMEOUT)
        .send()
        .await
        .ok()?;
    resp.json::<serde_json::Value>().await.ok()
}

/// Query the "latest version" to display for an npm tool: take `latest`, and when the local version leads,
/// also query the prerelease channels of that tool (see `npm_prerelease_tags`) — same dist-tags table, no extra request.
async fn fetch_npm_latest_for_tool(
    client: &reqwest::Client,
    package: &str,
    tool: &str,
    local_version: Option<&str>,
) -> Option<String> {
    let dist_tags = fetch_npm_dist_tags(client, package).await?;
    // Only grok pays for the full metadata again (333 KB for that package) to get the `versions` table.
    let metadata = match tool {
        "grok" => fetch_npm_metadata(client, package).await,
        _ => None,
    };
    let published_versions = metadata
        .as_ref()
        .and_then(|value| value.get("versions"))
        .and_then(|value| value.as_object());
    pick_latest_version(
        &dist_tags,
        npm_prerelease_tags(tool),
        local_version,
        published_versions,
    )
}

/// Per-request timeout for version probes. The global client is tuned for proxy forwarding (600s total /
/// 30s connect); reusing it for latest-version probes would make the Hermes card and the "refresh / upgrade all"
/// buttons wait forever when api.github.com / pypi.org is blocked or hangs after the handshake. A failed probe
/// can simply fall through to the next source or display "unknown".
const LATEST_PROBE_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(15);

/// Helper function to fetch latest version from GitHub releases
async fn fetch_github_latest_version(client: &reqwest::Client, repo: &str) -> Option<String> {
    let url = format!("https://api.github.com/repos/{repo}/releases/latest");
    let resp = client
        .get(&url)
        .header("User-Agent", "cc-switch")
        .header("Accept", "application/vnd.github+json")
        .timeout(LATEST_PROBE_TIMEOUT)
        .send()
        .await
        .ok()?;
    let json = resp.json::<serde_json::Value>().await.ok()?;
    github_release_version_from_json(&json)
}

/// Extract the version number to display from the JSON of a GitHub latest release.
///
/// Prefer a semantic version parsed out of the release `name`, falling back to `tag_name` (with the `v` prefix stripped):
/// Hermes tags are calendar-based (`v2026.8.31`), and the semantic version the CLI reports (`0.21.0`) only appears in
/// the name ("Hermes Agent v0.21.0 (v2026.8.31)"). Both paths go through `release_display_version`,
/// which filters calendar-style numbers: treated as a version they would parse fine in the frontend (2026 > 0)
/// and permanently claim "update available", and after upgrading the version would not change, triggering a
/// false "version unchanged" report. Better to return None than a non-semantic version so the caller can fall
/// back to another source. Rate limiting (a 403 JSON with only a message) also yields None.
fn github_release_version_from_json(json: &serde_json::Value) -> Option<String> {
    let from_name = json
        .get("name")
        .and_then(|v| v.as_str())
        .and_then(|name| release_display_version(&extract_version(name)));
    from_name.or_else(|| {
        json.get("tag_name")
            .and_then(|v| v.as_str())
            .and_then(|tag| release_display_version(tag.strip_prefix('v').unwrap_or(tag)))
    })
}

/// Only accept strings that parse as a semantic version and whose first segment does not look like a year (< 1000).
fn release_display_version(candidate: &str) -> Option<String> {
    match parse_semver(candidate) {
        Some(([major, ..], _)) if major < 1000 => Some(candidate.to_string()),
        _ => None,
    }
}

/// If the latest reported by a fallback source is already strictly behind the local version, do not display it
/// (return None) — showing a "latest version" older than the current one reproduces the "latest < current"
/// contradiction users reported. When either side is unparseable, assume no lead and display as usual.
fn drop_latest_behind_local(latest: Option<String>, local_version: Option<&str>) -> Option<String> {
    let latest = latest?;
    let local_leads = local_version
        .and_then(|local| compare_semver(local, &latest))
        .is_some_and(|ord| ord == std::cmp::Ordering::Greater);
    (!local_leads).then_some(latest)
}

/// Hermes "latest version": GitHub Releases first, PyPI as fallback.
///
/// cc-switch installs/upgrades Hermes via the official install.sh (`git clone` of the main branch) and
/// `hermes update` (`git pull`); nothing in that chain involves PyPI. The PyPI `hermes-agent` package has
/// been stale since 0.19.0 (2026-07-20) and upstream only publishes GitHub Releases
/// (#6475 / #6618 / #7033: "latest version" stuck at 0.19.0, older than the current one, and the upgrade button never appears).
/// PyPI is only used when GitHub is unreachable or rate limited, and its value is hidden when the local version already exceeds it — better to show "unknown".
async fn fetch_hermes_latest_version(
    client: &reqwest::Client,
    local_version: Option<&str>,
) -> Option<String> {
    if let Some(version) = fetch_github_latest_version(client, "NousResearch/hermes-agent").await {
        return Some(version);
    }
    let pypi = fetch_pypi_latest_version(client, "hermes-agent").await;
    drop_latest_behind_local(pypi, local_version)
}

/// Helper function to fetch latest version from PyPI
async fn fetch_pypi_latest_version(client: &reqwest::Client, package: &str) -> Option<String> {
    let url = format!("https://pypi.org/pypi/{package}/json");
    match client.get(&url).timeout(LATEST_PROBE_TIMEOUT).send().await {
        Ok(resp) => {
            if let Ok(json) = resp.json::<serde_json::Value>().await {
                json.get("info")
                    .and_then(|info| info.get("version"))
                    .and_then(|v| v.as_str())
                    .map(|s| s.to_string())
            } else {
                None
            }
        }
        Err(_) => None,
    }
}

/// Precompiled version number regex
static VERSION_RE: Lazy<Regex> =
    Lazy::new(|| Regex::new(r"\d+\.\d+\.\d+(-[\w.]+)?").expect("Invalid version regex"));

/// Extract the bare version number from version output
fn extract_version(raw: &str) -> String {
    VERSION_RE
        .find(raw)
        .map(|m| m.as_str().to_string())
        .unwrap_or_else(|| raw.to_string())
}

/// Unified error text for an uninstalled tool; the WSL path prefixes it with `[WSL:{distro}] `.
const NOT_INSTALLED: &str = "not installed or not executable";

/// Three-state result of a CLI version probe, unifying the return of every probe (`try_get_version` /
/// `try_get_version_wsl` / `scan_cli_version`) across platforms so `ToolVersion` can expose a structured
/// `installed_but_broken` signal instead of making the frontend infer semantics from error text.
///
/// The key distinction is "not installed" vs "installed but `--version` itself exits with an error" (e.g. the
/// tool requires a newer Node): the latter must be reported truthfully instead of masked by an older version found
/// elsewhere, otherwise "upgraded but cannot run" hides behind the old version and looks like "upgrade succeeded but the version did not change".
enum ShellProbe {
    /// Version obtained successfully
    Found(String),
    /// The executable exists but `--version` exited non-zero (carries diagnostics, e.g. the last lines of stderr)
    FoundButFailed(String),
    /// The command was not found (carries a descriptive message for the UI)
    NotFound(String),
}

/// Probe `{tool} --version` through the user shell on non-Windows platforms.
///
/// Windows does not take this path: `cmd /C {tool}` can accidentally trigger an App Execution Alias or a
/// protocol handler (which once disabled the whole Windows build), so there `scan_cli_version` only runs
/// the real executable it has already located.
#[cfg(not(target_os = "windows"))]
fn try_get_version(tool: &str) -> ShellProbe {
    use std::process::Command;

    let output = {
        let shell = std::env::var("SHELL")
            .ok()
            .filter(|s| is_valid_shell(s))
            .unwrap_or_else(|| "sh".to_string());
        let flag = default_flag_for_shell(&shell);
        Command::new(shell)
            .arg(flag)
            .arg(format!("{tool} --version"))
            .output()
    };

    match output {
        Ok(out) => {
            let stdout = decode_command_output(&out.stdout).trim().to_string();
            let stderr = decode_command_output(&out.stderr).trim().to_string();
            if out.status.success() {
                let raw = if stdout.is_empty() { &stderr } else { &stdout };
                if raw.is_empty() {
                    ShellProbe::NotFound(NOT_INSTALLED.to_string())
                } else {
                    ShellProbe::Found(extract_version(raw))
                }
            } else {
                // exit 127 = the shell could not find the command (safe to fall back to the search path); any other
                // non-zero code = the command exists but `--version` itself failed, which must be reported truthfully rather than masked.
                let err = if stderr.is_empty() { stdout } else { stderr };
                if out.status.code() == Some(127) || err.is_empty() {
                    ShellProbe::NotFound(NOT_INSTALLED.to_string())
                } else {
                    ShellProbe::FoundButFailed(last_lines(err.trim(), 4))
                }
            }
        }
        Err(_) => ShellProbe::NotFound(NOT_INSTALLED.to_string()),
    }
}

/// Validate that a WSL distribution name is well formed
/// WSL distribution names may only contain letters, digits, hyphens and underscores
#[cfg(target_os = "windows")]
fn is_valid_wsl_distro_name(name: &str) -> bool {
    !name.is_empty()
        && name.len() <= 64
        && name
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_' || c == '.')
}

/// Validate that the given shell name is one of the allowed shells.
fn is_valid_shell(shell: &str) -> bool {
    matches!(
        shell.rsplit('/').next().unwrap_or(shell),
        "sh" | "bash" | "zsh" | "fish" | "dash"
    )
}

/// Validate that the given shell flag is one of the allowed flags.
#[cfg(target_os = "windows")]
fn is_valid_shell_flag(flag: &str) -> bool {
    matches!(flag, "-c" | "-lc" | "-lic")
}

/// Return the default invocation flag for the given shell.
fn default_flag_for_shell(shell: &str) -> &'static str {
    match shell.rsplit('/').next().unwrap_or(shell) {
        "dash" | "sh" => "-c",
        "fish" => "-lc",
        _ => "-lic",
    }
}

#[cfg(target_os = "windows")]
fn try_get_version_wsl(
    tool: &str,
    distro: &str,
    force_shell: Option<&str>,
    force_shell_flag: Option<&str>,
) -> ShellProbe {
    use std::process::Command;

    // Defensive assertion: tool may only be one of the predefined values
    debug_assert!(VALID_TOOLS.contains(&tool), "unexpected tool name: {tool}");

    // Validate the distro name to prevent command injection
    if !is_valid_wsl_distro_name(distro) {
        return ShellProbe::NotFound(format!("[WSL:{distro}] invalid distro name"));
    }

    // Build the shell script detection logic
    let (shell, flag, cmd) = if let Some(shell) = force_shell {
        // Defensive validation: never allow an arbitrary executable name here.
        if !is_valid_shell(shell) {
            return ShellProbe::NotFound(format!("[WSL:{distro}] invalid shell: {shell}"));
        }
        let shell = shell.rsplit('/').next().unwrap_or(shell);
        let flag = if let Some(flag) = force_shell_flag {
            if !is_valid_shell_flag(flag) {
                return ShellProbe::NotFound(format!("[WSL:{distro}] invalid shell flag: {flag}"));
            }
            flag
        } else {
            default_flag_for_shell(shell)
        };

        (shell.to_string(), flag, format!("{tool} --version"))
    } else {
        let cmd = if let Some(flag) = force_shell_flag {
            if !is_valid_shell_flag(flag) {
                return ShellProbe::NotFound(format!("[WSL:{distro}] invalid shell flag: {flag}"));
            }
            format!("\"${{SHELL:-sh}}\" {flag} '{tool} --version'")
        } else {
            // Fallback: try -lic, -lc, -c automatically
            format!(
                "\"${{SHELL:-sh}}\" -lic '{tool} --version' 2>/dev/null || \"${{SHELL:-sh}}\" -lc '{tool} --version' 2>/dev/null || \"${{SHELL:-sh}}\" -c '{tool} --version'"
            )
        };

        ("sh".to_string(), "-c", cmd)
    };

    let output = Command::new("wsl.exe")
        .args(["-d", distro, "--", &shell, flag, &cmd])
        .creation_flags(CREATE_NO_WINDOW)
        .output();

    match output {
        Ok(out) => {
            let stdout = decode_command_output(&out.stdout).trim().to_string();
            let stderr = decode_command_output(&out.stderr).trim().to_string();
            if out.status.success() {
                let raw = if stdout.is_empty() { &stderr } else { &stdout };
                if raw.is_empty() {
                    ShellProbe::NotFound(format!("[WSL:{distro}] {NOT_INSTALLED}"))
                } else {
                    ShellProbe::Found(extract_version(raw))
                }
            } else {
                let err = if stderr.is_empty() { stdout } else { stderr };
                // The exit code passed through by wsl.exe is not always reliable, so "not installed" is also
                // detected via exit 127 and the "command not found" text; other non-zero exits count as "installed but --version failed".
                let not_found = err.is_empty()
                    || out.status.code() == Some(127)
                    || err.contains("command not found")
                    || err.contains("not found");
                if not_found {
                    ShellProbe::NotFound(format!("[WSL:{distro}] {NOT_INSTALLED}"))
                } else {
                    ShellProbe::FoundButFailed(format!(
                        "[WSL:{distro}] {}",
                        last_lines(err.trim(), 4)
                    ))
                }
            }
        }
        Err(e) => ShellProbe::NotFound(format!("[WSL:{distro}] exec failed: {e}")),
    }
}

/// Stub for WSL version detection on non-Windows platforms
/// Note: this function is never actually called, because `wsl_distro_from_path` always returns None off Windows.
/// It is kept for API consistency so future refactors do not miss it.
#[cfg(not(target_os = "windows"))]
fn try_get_version_wsl(
    _tool: &str,
    _distro: &str,
    _force_shell: Option<&str>,
    _force_shell_flag: Option<&str>,
) -> ShellProbe {
    ShellProbe::NotFound("WSL check not supported on this platform".to_string())
}

fn push_unique_path(paths: &mut Vec<std::path::PathBuf>, path: std::path::PathBuf) {
    if path.as_os_str().is_empty() {
        return;
    }

    if !paths.iter().any(|existing| existing == &path) {
        paths.push(path);
    }
}

fn push_env_single_dir(paths: &mut Vec<std::path::PathBuf>, value: Option<std::ffi::OsString>) {
    if let Some(raw) = value {
        push_unique_path(paths, std::path::PathBuf::from(raw));
    }
}

fn extend_from_path_list(
    paths: &mut Vec<std::path::PathBuf>,
    value: Option<std::ffi::OsString>,
    suffix: Option<&str>,
) {
    if let Some(raw) = value {
        for p in std::env::split_paths(&raw) {
            let dir = match suffix {
                Some(s) => p.join(s),
                None => p,
            };
            push_unique_path(paths, dir);
        }
    }
}

fn extend_from_cli_path_env(
    paths: &mut Vec<std::path::PathBuf>,
    value: Option<std::ffi::OsString>,
) {
    if let Some(raw) = value {
        for p in std::env::split_paths(&raw) {
            if should_skip_cli_path_env_dir(&p) {
                continue;
            }
            push_unique_path(paths, p);
        }
    }
}

fn should_skip_cli_path_env_dir(path: &Path) -> bool {
    #[cfg(target_os = "windows")]
    {
        is_windows_app_execution_alias_dir(path)
    }

    #[cfg(not(target_os = "windows"))]
    {
        let _ = path;
        false
    }
}

#[cfg(target_os = "windows")]
fn is_windows_app_execution_alias_dir(path: &Path) -> bool {
    let normalized = path
        .to_string_lossy()
        .replace('/', "\\")
        .to_ascii_lowercase();
    normalized
        .trim_end_matches('\\')
        .ends_with("\\microsoft\\windowsapps")
}

#[cfg_attr(not(target_os = "windows"), allow(dead_code))]
fn push_env_child_dir(
    paths: &mut Vec<std::path::PathBuf>,
    value: Option<std::ffi::OsString>,
    child: &str,
) {
    if let Some(raw) = value {
        push_unique_path(paths, std::path::PathBuf::from(raw).join(child));
    }
}

#[cfg_attr(not(target_os = "windows"), allow(dead_code))]
fn extend_existing_child_search_paths(
    paths: &mut Vec<std::path::PathBuf>,
    base: &Path,
    suffix: Option<&str>,
) {
    if !base.exists() {
        return;
    }

    if let Ok(entries) = std::fs::read_dir(base) {
        for entry in entries.flatten() {
            let path = match suffix {
                Some(suffix) => entry.path().join(suffix),
                None => entry.path(),
            };
            if path.exists() {
                push_unique_path(paths, path);
            }
        }
    }
}

#[cfg(target_os = "windows")]
fn extend_windows_cli_manager_search_paths(paths: &mut Vec<std::path::PathBuf>, home: &Path) {
    push_env_single_dir(paths, std::env::var_os("PNPM_HOME"));
    push_env_child_dir(paths, std::env::var_os("VOLTA_HOME"), "bin");
    push_env_single_dir(paths, std::env::var_os("NVM_SYMLINK"));
    push_env_child_dir(paths, std::env::var_os("SCOOP"), "shims");
    push_env_child_dir(paths, std::env::var_os("SCOOP_GLOBAL"), "shims");

    if let Some(nvm_home) = std::env::var_os("NVM_HOME") {
        let nvm_home = std::path::PathBuf::from(nvm_home);
        push_unique_path(paths, nvm_home.clone());
        extend_existing_child_search_paths(paths, &nvm_home, None);
    }

    if let Some(appdata) = dirs::data_dir() {
        let nvm_home = appdata.join("nvm");
        push_unique_path(paths, nvm_home.clone());
        extend_existing_child_search_paths(paths, &nvm_home, None);
    }

    if !home.as_os_str().is_empty() {
        push_unique_path(paths, home.join("scoop").join("shims"));
    }

    if let Some(local_data) = dirs::data_local_dir() {
        push_unique_path(paths, local_data.join("pnpm"));
        push_unique_path(paths, local_data.join("Volta").join("bin"));
        push_unique_path(paths, local_data.join("Yarn").join("bin"));
    }

    let program_data = std::env::var_os("ProgramData")
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|| std::path::PathBuf::from("C:\\ProgramData"));
    push_unique_path(paths, program_data.join("scoop").join("shims"));
}

/// Path priority of the OpenCode install.sh (see the https://github.com/anomalyco/opencode README):
///   $OPENCODE_INSTALL_DIR > $XDG_BIN_DIR > $HOME/bin > $HOME/.opencode/bin
/// Also scans the default Bun global install path (~/.bun/bin)
/// and the Go install paths (~/go/bin, $GOPATH/*/bin).
fn opencode_extra_search_paths(
    home: &Path,
    opencode_install_dir: Option<std::ffi::OsString>,
    xdg_bin_dir: Option<std::ffi::OsString>,
    gopath: Option<std::ffi::OsString>,
) -> Vec<std::path::PathBuf> {
    let mut paths = Vec::new();

    push_env_single_dir(&mut paths, opencode_install_dir);
    push_env_single_dir(&mut paths, xdg_bin_dir);

    if !home.as_os_str().is_empty() {
        push_unique_path(&mut paths, home.join("bin"));
        push_unique_path(&mut paths, home.join(".opencode").join("bin"));
        push_unique_path(&mut paths, home.join(".bun").join("bin"));
        push_unique_path(&mut paths, home.join("go").join("bin"));
    }

    extend_from_path_list(&mut paths, gopath, Some("bin"));

    paths
}

/// Grok's official installer writes the launcher to `$GROK_BIN_DIR` or, by
/// default, `~/.grok/bin`. Keep these ahead of generic npm/Node locations so
/// version probing and anchored updates can see the native distribution even
/// when the GUI process inherited a stale PATH.
fn grok_extra_search_paths(
    home: &Path,
    grok_bin_dir: Option<std::ffi::OsString>,
) -> Vec<std::path::PathBuf> {
    let mut paths = Vec::new();
    push_env_single_dir(&mut paths, grok_bin_dir);
    if !home.as_os_str().is_empty() {
        push_unique_path(&mut paths, home.join(".grok").join("bin"));
    }
    paths
}

fn tool_executable_candidates(tool: &str, dir: &Path) -> Vec<std::path::PathBuf> {
    #[cfg(target_os = "windows")]
    {
        let extensionless = dir.join(tool);
        let mut candidates = vec![
            dir.join(format!("{tool}.cmd")),
            dir.join(format!("{tool}.exe")),
        ];
        if windows_runnable_sibling_for_extensionless_tool(&extensionless).is_none() {
            candidates.push(extensionless);
        }
        candidates
    }

    #[cfg(not(target_os = "windows"))]
    {
        vec![dir.join(tool)]
    }
}

fn extend_mise_node_search_paths(paths: &mut Vec<std::path::PathBuf>, home: &Path) {
    if home.as_os_str().is_empty() {
        return;
    }

    let mise_base = home.join(".local/share/mise");
    push_unique_path(paths, mise_base.join("shims"));

    let node_installs = mise_base.join("installs").join("node");
    if node_installs.exists() {
        if let Ok(entries) = std::fs::read_dir(&node_installs) {
            for entry in entries.flatten() {
                let bin_path = entry.path().join("bin");
                if bin_path.exists() {
                    push_unique_path(paths, bin_path);
                }
            }
        }
    }
}

/// The "effective PATH" used during detection. On Windows the inherited
/// process PATH can be incomplete — most notably after an in-app self-update,
/// where the MSI/WiX-auto-launched process inherits only the machine-level PATH
/// and drops the user-level PATH (see #6061). Any CLI installed in a user-PATH
/// location (winget Claude `%LOCALAPPDATA%\Programs\claude`, the standalone
/// Codex installer `%LOCALAPPDATA%\Programs\OpenAI\Codex\bin`, a custom npm
/// prefix `D:\npm-global`, …) then reads as "not installed".
///
/// Reconstruct the effective PATH by merging the process PATH with the machine
/// and user registry PATH values (`REG_EXPAND_SZ` expanded), so detection sees
/// the same installations a freshly logged-in shell would — regardless of how
/// the current process was launched. Process entries are kept first (a runtime
/// override wins); registry entries fill whatever the process is missing;
/// duplicates are removed.
///
/// See `env_checker::check_system_env` for the same set of registry keys; here
/// we read only the `Path` value.
#[cfg(target_os = "windows")]
fn effective_path_string() -> String {
    use winreg::enums::{HKEY_CURRENT_USER, HKEY_LOCAL_MACHINE};
    use winreg::RegKey;

    let process = std::env::var("PATH").unwrap_or_default();
    let user = RegKey::predef(HKEY_CURRENT_USER)
        .open_subkey("Environment")
        .and_then(|k| k.get_value::<String, &str>("Path"))
        .map(|raw| expand_env_chars(&raw))
        .unwrap_or_default();
    let machine = RegKey::predef(HKEY_LOCAL_MACHINE)
        .open_subkey("SYSTEM\\CurrentControlSet\\Control\\Session Manager\\Environment")
        .and_then(|k| k.get_value::<String, &str>("Path"))
        .map(|raw| expand_env_chars(&raw))
        .unwrap_or_default();
    merge_path_segments_win(&[&process, &user, &machine])
}

/// `extend_from_cli_path_env` consumes an `OsString` via `split_paths`. On
/// Windows this is derived from `effective_path_string`; on other platforms the
/// raw process value is returned unchanged (zero behaviour change).
#[cfg(target_os = "windows")]
fn effective_path_os() -> Option<std::ffi::OsString> {
    Some(std::ffi::OsString::from(effective_path_string()))
}

#[cfg(not(target_os = "windows"))]
fn effective_path_os() -> Option<std::ffi::OsString> {
    std::env::var_os("PATH")
}

/// Narrow read-only bridge for the owner probe: use the same effective PATH as
/// CLI enumeration (including the Windows user/machine registry merge) without
/// exposing the legacy detection implementation outside the crate.
pub(crate) fn effective_path_for_tool_detection() -> Option<std::ffi::OsString> {
    effective_path_os()
}

/// Prepend a candidate directory without converting the existing PATH to
/// UTF-8. Unix permits arbitrary non-NUL bytes in environment values; keeping
/// this as an `OsString` ensures one non-Unicode segment cannot discard or
/// corrupt every other interpreter directory needed by an npm/python shim.
#[cfg(not(target_os = "windows"))]
fn prepend_search_dir_to_path(dir: &Path, current_path: &std::ffi::OsStr) -> std::ffi::OsString {
    let mut path = dir.as_os_str().to_os_string();
    if !current_path.is_empty() {
        path.push(":");
        path.push(current_path);
    }
    path
}

/// Expand `%VAR%` environment-variable references. The registry `Path` value is
/// `REG_EXPAND_SZ`, and the `String` returned by `winreg` is not auto-expanded.
/// Variables such as `%LOCALAPPDATA%` / `%USERPROFILE%` / `%SystemRoot%` are
/// still defined in a process that lost its user PATH (Winlogon injects them
/// from the user profile), so expanding each via `std::env::var` is safe.
/// Undefined variables are preserved verbatim (no characters dropped).
#[cfg(target_os = "windows")]
fn expand_env_chars(raw: &str) -> String {
    let mut out = String::with_capacity(raw.len());
    let mut rest = raw;
    while let Some(open) = rest.find('%') {
        out.push_str(&rest[..open]);
        let after = &rest[open + 1..];
        match after.find('%') {
            None => {
                out.push('%');
                out.push_str(after);
                rest = "";
                break;
            }
            Some(close) => {
                let name = &after[..close];
                let is_ident =
                    !name.is_empty() && name.chars().all(|c| c.is_ascii_alphanumeric() || c == '_');
                if is_ident {
                    match std::env::var(name) {
                        Ok(val) => out.push_str(&val),
                        Err(_) => {
                            out.push('%');
                            out.push_str(name);
                            out.push('%');
                        }
                    }
                } else {
                    out.push('%');
                    out.push_str(name);
                    out.push('%');
                }
                rest = &after[close + 1..];
            }
        }
    }
    out.push_str(rest);
    out
}

/// Merge several Windows PATH strings (`;`-separated) in order, keeping only
/// the first occurrence of each segment (case-insensitive). Process segments
/// come first to respect runtime overrides, followed by user- and
/// machine-level registry segments.
#[cfg(target_os = "windows")]
fn merge_path_segments_win(parts: &[&str]) -> String {
    let mut seen: std::collections::HashSet<String> = std::collections::HashSet::new();
    let mut merged: Vec<&str> = Vec::new();
    for part in parts {
        for seg in part.split(';') {
            let s = seg.trim();
            if s.is_empty() || !seen.insert(s.to_ascii_lowercase()) {
                continue;
            }
            merged.push(s);
        }
    }
    merged.join(";")
}

/// Build the candidate search directories for a tool (native installs first, PATH as fallback).
/// Shared by the single-probe fallback (`scan_cli_version`) and the full enumeration
/// (`enumerate_tool_installations`) so both paths see the same set of install locations.
fn build_tool_search_paths(tool: &str) -> Vec<std::path::PathBuf> {
    let home = dirs::home_dir().unwrap_or_default();

    // Common install paths (native installs first)
    let mut search_paths: Vec<std::path::PathBuf> = Vec::new();
    if tool == "kimi" && !home.as_os_str().is_empty() {
        let configured_home = std::env::var_os("KIMI_CODE_HOME")
            .filter(|value| !value.is_empty())
            .map(std::path::PathBuf::from)
            .filter(|path| path.is_absolute())
            .unwrap_or_else(|| home.join(".kimi-code"));
        push_unique_path(&mut search_paths, configured_home.join("bin"));
    }
    if tool == "grok" {
        let extra_paths = grok_extra_search_paths(&home, std::env::var_os("GROK_BIN_DIR"));
        for path in extra_paths {
            push_unique_path(&mut search_paths, path);
        }
    }
    if !home.as_os_str().is_empty() {
        push_unique_path(&mut search_paths, home.join(".local/bin"));
        push_unique_path(&mut search_paths, home.join(".npm-global/bin"));
        push_unique_path(&mut search_paths, home.join("n/bin"));
        push_unique_path(&mut search_paths, home.join(".volta/bin"));
        extend_mise_node_search_paths(&mut search_paths, &home);
    }

    #[cfg(target_os = "macos")]
    {
        push_unique_path(
            &mut search_paths,
            std::path::PathBuf::from("/opt/homebrew/bin"),
        );
        push_unique_path(
            &mut search_paths,
            std::path::PathBuf::from("/usr/local/bin"),
        );
        if tool == "hermes" {
            let python_base = home.join("Library").join("Python");
            if python_base.exists() {
                if let Ok(entries) = std::fs::read_dir(&python_base) {
                    for entry in entries.flatten() {
                        let bin_path = entry.path().join("bin");
                        if bin_path.exists() {
                            push_unique_path(&mut search_paths, bin_path);
                        }
                    }
                }
            }
        }
    }

    #[cfg(target_os = "linux")]
    {
        push_unique_path(
            &mut search_paths,
            std::path::PathBuf::from("/usr/local/bin"),
        );
        push_unique_path(&mut search_paths, std::path::PathBuf::from("/usr/bin"));
    }

    #[cfg(target_os = "windows")]
    {
        // Official standalone (non-npm) installer locations — belt-and-suspenders
        // alongside the registry-PATH merge in `effective_path_os`. These
        // installers normally register themselves on the user PATH, but some
        // per-user MSI/MSIX installs do not, and an in-app-update relaunch can
        // drop the user PATH (#6061), so add them explicitly here. Placed ahead
        // of the npm directory so a native install wins over a stale npm shim
        // (#4701).
        if let Some(local_data) = dirs::data_local_dir() {
            if tool == "codex" {
                // OpenAI Codex Installer.exe / .msi standalone install location
                // (#6061, #6047).
                push_unique_path(
                    &mut search_paths,
                    local_data
                        .join("Programs")
                        .join("OpenAI")
                        .join("Codex")
                        .join("bin"),
                );
            }
            if tool == "claude" {
                // `winget install Anthropic.ClaudeCode` / official native
                // installer location (#6278).
                push_unique_path(
                    &mut search_paths,
                    local_data.join("Programs").join("claude"),
                );
            }
        }
        if let Some(appdata) = dirs::data_dir() {
            push_unique_path(&mut search_paths, appdata.join("npm"));
            if tool == "hermes" {
                let python_base = appdata.join("Python");
                if python_base.exists() {
                    if let Ok(entries) = std::fs::read_dir(&python_base) {
                        for entry in entries.flatten() {
                            let scripts_path = entry.path().join("Scripts");
                            if scripts_path.exists() {
                                push_unique_path(&mut search_paths, scripts_path);
                            }
                        }
                    }
                }
            }
        }
        if tool == "hermes" {
            if let Some(local_data) = dirs::data_local_dir() {
                let programs_python = local_data.join("Programs").join("Python");
                if programs_python.exists() {
                    if let Ok(entries) = std::fs::read_dir(&programs_python) {
                        for entry in entries.flatten() {
                            let scripts_path = entry.path().join("Scripts");
                            if scripts_path.exists() {
                                push_unique_path(&mut search_paths, scripts_path);
                            }
                        }
                    }
                }
            }
        }
        push_unique_path(
            &mut search_paths,
            std::path::PathBuf::from("C:\\Program Files\\nodejs"),
        );
        extend_windows_cli_manager_search_paths(&mut search_paths, &home);
    }

    let fnm_base = home.join(".local/state/fnm_multishells");
    if fnm_base.exists() {
        if let Ok(entries) = std::fs::read_dir(&fnm_base) {
            for entry in entries.flatten() {
                let bin_path = entry.path().join("bin");
                if bin_path.exists() {
                    push_unique_path(&mut search_paths, bin_path);
                }
            }
        }
    }

    let nvm_base = home.join(".nvm/versions/node");
    if nvm_base.exists() {
        if let Ok(entries) = std::fs::read_dir(&nvm_base) {
            for entry in entries.flatten() {
                let bin_path = entry.path().join("bin");
                if bin_path.exists() {
                    push_unique_path(&mut search_paths, bin_path);
                }
            }
        }
    }

    if tool == "opencode" {
        let extra_paths = opencode_extra_search_paths(
            &home,
            std::env::var_os("OPENCODE_INSTALL_DIR"),
            std::env::var_os("XDG_BIN_DIR"),
            std::env::var_os("GOPATH"),
        );

        for path in extra_paths {
            push_unique_path(&mut search_paths, path);
        }
    }

    let path_env = effective_path_os();
    extend_from_cli_path_env(&mut search_paths, path_env);
    search_paths
}

#[cfg(target_os = "windows")]
fn is_windows_command_script(path: &Path) -> bool {
    path.extension()
        .and_then(|ext| ext.to_str())
        .map(|ext| ext.eq_ignore_ascii_case("cmd") || ext.eq_ignore_ascii_case("bat"))
        .unwrap_or(false)
}

#[cfg(target_os = "windows")]
fn windows_runnable_sibling_for_extensionless_tool(path: &Path) -> Option<std::path::PathBuf> {
    if path.extension().is_some() {
        return None;
    }

    ["cmd", "exe"]
        .iter()
        .map(|ext| path.with_extension(ext))
        .find(|candidate| candidate.is_file())
}

#[cfg(target_os = "windows")]
fn build_windows_tool_command(
    tool_path: &Path,
    args: &[&str],
    new_path: &str,
) -> std::process::Command {
    use std::process::Command;

    if is_windows_command_script(tool_path) {
        // `resolve_path_default` returns a canonical path so callers can
        // compare installation identities. Canonical Windows paths carry a
        // `\\?\` prefix, which `cmd /C call` rejects for batch files. Normalize
        // only at this shell boundary and keep the canonical identity intact
        // everywhere else.
        let shell_path = crate::platform::windows_shell_compatible_path(tool_path);
        let path = shell_path.to_string_lossy();
        let args = args
            .iter()
            .map(|arg| windows_cmd_double_quote_arg(arg))
            .collect::<Vec<_>>()
            .join(" ");
        let command = format!(
            "call {}{}",
            win_quote_path_for_batch(&path),
            if args.is_empty() {
                String::new()
            } else {
                format!(" {args}")
            }
        );
        let mut cmd = Command::new("cmd");
        cmd.args(["/D", "/S", "/C"])
            .raw_arg(&command)
            .env("PATH", new_path)
            .creation_flags(CREATE_NO_WINDOW);
        return cmd;
    }

    let mut cmd = Command::new(tool_path);
    cmd.args(args)
        .env("PATH", new_path)
        .creation_flags(CREATE_NO_WINDOW);
    cmd
}

#[cfg(target_os = "windows")]
fn run_windows_tool_command(
    tool_path: &Path,
    args: &[&str],
    new_path: &str,
) -> std::io::Result<std::process::Output> {
    build_windows_tool_command(tool_path, args, new_path).output()
}

#[cfg(target_os = "windows")]
fn run_windows_tool_version_command(
    tool_path: &Path,
    new_path: &str,
) -> std::io::Result<std::process::Output> {
    run_windows_tool_command(tool_path, &["--version"], new_path)
}

/// Probe the version of the PATH-default entry on Windows. Uses
/// `resolve_path_default` (which merges the registry PATH and filters out
/// App Execution Aliases) to get the executable the user actually runs, then
/// runs `--version` on it. Consumed by `get_single_tool_version_impl` before
/// the directory scan, so the displayed version matches what `tool` resolves
/// to in a terminal (#4701).
///
/// Returns `NotFound` when no entry is resolved on PATH (the caller falls back
/// to `scan_cli_version` over the hardcoded/registry dirs); `FoundButFailed`
/// when an entry was resolved but `--version` exited non-zero (installed but
/// not runnable), which is reported as-is without falling back, so an old
/// install elsewhere cannot mask a broken default.
#[cfg(target_os = "windows")]
fn probe_path_default_version(tool: &str) -> ShellProbe {
    let path_default = match resolve_path_default(tool, None) {
        Ok(Some(p)) => p,
        _ => return ShellProbe::NotFound(NOT_INSTALLED.to_string()),
    };
    let current_path = effective_path_string();
    match run_windows_tool_version_command(&path_default, &current_path) {
        Ok(out) => {
            let stdout = decode_command_output(&out.stdout).trim().to_string();
            let stderr = decode_command_output(&out.stderr).trim().to_string();
            if out.status.success() {
                let raw = if stdout.is_empty() { &stderr } else { &stdout };
                if raw.is_empty() {
                    ShellProbe::NotFound(NOT_INSTALLED.to_string())
                } else {
                    ShellProbe::Found(extract_version(raw))
                }
            } else {
                let err = if stderr.is_empty() { stdout } else { stderr };
                if err.is_empty() {
                    ShellProbe::NotFound(NOT_INSTALLED.to_string())
                } else {
                    ShellProbe::FoundButFailed(last_lines(err.trim(), 4))
                }
            }
        }
        Err(_) => ShellProbe::NotFound(NOT_INSTALLED.to_string()),
    }
}

/// Scan the common paths for the CLI (single-probe fallback when the main PATH command misses).
fn scan_cli_version(tool: &str) -> ShellProbe {
    #[cfg(not(target_os = "windows"))]
    use std::process::Command;

    let search_paths = build_tool_search_paths(tool);
    #[cfg(target_os = "windows")]
    let current_path = effective_path_string();
    #[cfg(not(target_os = "windows"))]
    let current_path = effective_path_os().unwrap_or_default();

    // Record the first diagnostic for "the executable exists but `--version` exited non-zero".
    // Typical case: the tool is installed but cannot run in the current environment (e.g. openclaw requires Node v22.19+).
    // That is far more useful than a generic "not installed" and is returned when the loop ends without a version.
    let mut exec_diagnostic: Option<String> = None;

    for path in &search_paths {
        #[cfg(target_os = "windows")]
        let new_path = format!("{};{}", path.display(), current_path);

        #[cfg(not(target_os = "windows"))]
        let new_path = prepend_search_dir_to_path(path, &current_path);

        for tool_path in tool_executable_candidates(tool, path) {
            if !tool_path.exists() {
                continue;
            }

            #[cfg(target_os = "windows")]
            let output = run_windows_tool_version_command(&tool_path, &new_path);

            #[cfg(not(target_os = "windows"))]
            let output = {
                Command::new(&tool_path)
                    .arg("--version")
                    .env("PATH", &new_path)
                    .output()
            };

            if let Ok(out) = output {
                let stdout = decode_command_output(&out.stdout).trim().to_string();
                let stderr = decode_command_output(&out.stderr).trim().to_string();
                if out.status.success() {
                    let raw = if stdout.is_empty() { &stderr } else { &stdout };
                    if !raw.is_empty() {
                        return ShellProbe::Found(extract_version(raw));
                    }
                } else if exec_diagnostic.is_none() {
                    let detail = if stderr.is_empty() { stdout } else { stderr };
                    let detail = detail.trim();
                    if !detail.is_empty() {
                        exec_diagnostic = Some(last_lines(detail, 4));
                    }
                }
            }
        }
    }

    // A diagnostic means an executable was found but `--version` failed (installed yet unusable); otherwise treat it as not installed.
    match exec_diagnostic {
        Some(detail) => ShellProbe::FoundButFailed(detail),
        None => ShellProbe::NotFound(NOT_INSTALLED.to_string()),
    }
}

/// One installation of a tool on the system, used to diagnose conflicts between multiple installs.
/// The fields stay snake_case (matching `ToolVersion`) and the frontend reads them by the same names.
#[derive(Debug, serde::Serialize)]
pub struct ToolInstallation {
    /// Candidate entry path (the one the user actually sees/types on PATH, symlinks unresolved).
    pub(crate) path: String,
    /// The version parsed when `--version` succeeded.
    pub(crate) version: Option<String>,
    /// Whether `--version` exited 0 (installed and runnable in the current environment).
    pub(crate) runnable: bool,
    /// The last lines of diagnostics when it cannot run.
    pub(crate) error: Option<String>,
    /// Install source inferred from the path prefix (nvm/homebrew/...), which drives the UI badge.
    pub(crate) source: String,
    /// Whether this is the one PATH resolves to (= the command line default, and the target an upgrade affects).
    pub(crate) is_path_default: bool,
    /// The real path after canonicalize (brew looks like `Cellar/<formula>/...`, a native claude install like
    /// `~/.local/share/claude/versions/...`), used by `anchored_command_from_paths` to identify the real target.
    /// `enumerate_tool_installations` already computed it for deduplication, so reusing it here keeps
    /// `installs_anchored_command` upstream from canonicalizing again — removing a redundant syscall and
    /// closing the "enumerate and anchor see the same real file" consistency boundary (otherwise a symlink
    /// swapped between the two canonicalize calls would anchor to a different target). `#[serde(skip)]` keeps it off the frontend.
    #[serde(skip)]
    pub(crate) real: std::path::PathBuf,
}

/// Infer the install source from the executable path prefix. Pure string matching, no side effects.
/// Order sensitive: Homebrew's Cellar real path must match before the generic rules.
pub(crate) fn infer_install_source(path: &Path) -> &'static str {
    let s = path
        .to_string_lossy()
        .replace('\\', "/")
        .to_ascii_lowercase();
    if s.contains("/.nvm/") {
        "nvm"
    } else if s.contains("/homebrew/") || s.contains("/cellar/") {
        "homebrew"
    // `.volta` is the macOS/Linux default install (`~/.volta/bin`); the `/volta/` fallback covers
    // Windows `%LOCALAPPDATA%\Volta\bin` / `%VOLTA_HOME%\bin` (no leading dot).
    } else if s.contains("/.volta/") || s.contains("/volta/") {
        "volta"
    } else if s.contains("fnm_multishells") {
        "fnm"
    } else if s.contains("/mise/") {
        "mise"
    } else if s.contains("/.bun/") {
        "bun"
    // pnpm global package directory: macOS usually `~/.local/share/pnpm` (normalized to `/pnpm/`)
    // and Windows `%LOCALAPPDATA%\pnpm` / `%PNPM_HOME%` both match `/pnpm/`.
    } else if s.contains("/pnpm/") {
        "pnpm"
    } else if s.contains("/scoop/") {
        "scoop"
    } else if s.contains("/library/python")
        || s.contains("/scripts/")
        || s.contains("/site-packages/")
    {
        "pip"
    } else {
        "system"
    }
}

/// Pick the first absolute path line from shell output (starts with `/` after trimming), skipping the
/// welcome banners and prompts printed by .zshrc in an interactive login shell (`-lic`). The caller does the canonicalize (it touches the FS).
#[cfg(not(target_os = "windows"))]
fn first_abs_path_line(raw: &str) -> Option<&str> {
    raw.lines().map(str::trim).find(|l| l.starts_with('/'))
}

/// Take the value of the `PATH=` line from `env` output. The value must start with `/` — the first PATH
/// segment is always an absolute path, and that constraint also skips the case where some multi-line
/// environment variable happens to have a line starting with `PATH=`, the same way `first_abs_path_line` tolerates interactive shell noise.
#[cfg(not(target_os = "windows"))]
fn path_line_from_env_output(raw: &str) -> Option<&str> {
    raw.lines()
        .filter_map(|line| line.strip_prefix("PATH="))
        .find(|value| value.starts_with('/'))
}

/// Merge two PATHs: all of `primary` stays in front, and the segments of `extra` not already present are appended in order.
/// Empty segments are dropped (in POSIX an empty segment in `a::b` means the current directory, which must not be injected).
#[cfg(not(target_os = "windows"))]
pub(crate) fn merge_path_segments(primary: &str, extra: &str) -> String {
    let mut seen = std::collections::HashSet::new();
    let mut merged: Vec<&str> = Vec::new();
    for segment in primary.split(':').chain(extra.split(':')) {
        if segment.is_empty() || !seen.insert(segment) {
            continue;
        }
        merged.push(segment);
    }
    merged.join(":")
}

/// Resolve the real user PATH with the same login shell as `resolve_path_default`, for
/// `run_tool_lifecycle_silently` to inject into install/upgrade scripts.
///
/// **The asymmetry this fixes**: the probing stage (`try_get_version` / `resolve_path_default`) runs
/// `$SHELL -lic`, which reads `.zshrc`/`.zprofile` and therefore sees nvm / homebrew / volta, while the
/// execution stage is a non-login `bash -c` that inherits the PATH launchd gives a GUI app, usually just
/// `/usr/bin:/bin:/usr/sbin:/sbin`. An anchored command calls the executable by absolute path and is not
/// affected, but two cases still slip through:
/// 1. **The executable spawns a third-party CLI internally**: `grok update` uses `npm view` to look up the
///    latest version (see `grok_native_update_command`), and npm itself is a `#!/usr/bin/env node` script;
///    any future self-update that calls node/git/python internally is the same.
/// 2. **The install branch `<official installer> || npm i -g <pkg>@latest`**: the right side of `||` is a bare
///    command that always exits 127 under a narrow PATH, so there is effectively no fallback.
///
/// Raising the execution-stage PATH to the probing-stage level removes both at once.
///
/// **Why `/usr/bin/env` instead of `echo $PATH`**: in fish `$PATH` is a list type and `"$PATH"` expands
/// space separated rather than colon separated, whereas `env` prints the real environment of the child
/// process, correctly formatted under any shell. Using the absolute path also bypasses the triple
/// uncertainty of alias / function / PATH (an interactive shell loads user aliases).
///
/// Returns `None` when it cannot be resolved, so the caller keeps its previous behaviour (no injection) and no new failure mode is introduced.
#[cfg(not(target_os = "windows"))]
pub(crate) fn login_shell_path() -> Option<String> {
    use std::process::Command;
    let shell = std::env::var("SHELL")
        .ok()
        .filter(|s| is_valid_shell(s))
        .unwrap_or_else(|| "sh".to_string());
    let flag = default_flag_for_shell(&shell);
    let out = Command::new(shell)
        .arg(flag)
        .arg("/usr/bin/env")
        .output()
        .ok()?;
    if !out.status.success() {
        return None;
    }
    let raw = decode_command_output(&out.stdout);
    Some(path_line_from_env_output(&raw)?.to_string())
}

/// Resolve the executable path PATH resolves to, using the same login shell as `try_get_version`, and
/// canonicalize it as the anchor for "command line default / upgrade target" (aligned with the install an upgrade affects).
#[cfg(not(target_os = "windows"))]
fn resolve_path_default(
    tool: &str,
    deadline: Option<CommandDeadline>,
) -> Result<Option<std::path::PathBuf>, String> {
    use std::process::{Command, Stdio};

    let shell = std::env::var("SHELL")
        .ok()
        .filter(|s| is_valid_shell(s))
        .unwrap_or_else(|| "sh".to_string());
    let flag = default_flag_for_shell(&shell);
    let mut cmd = Command::new(shell);
    cmd.arg(flag)
        .arg(format!("command -v {tool}"))
        // After switching to spawn, stdin is no longer defaulted to null the way output() does it, so it
        // must be closed explicitly: an inherited stdin may be a terminal/pipe and a read in an interactive rc would block forever.
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    isolate_child_process_group(&mut cmd);
    let child = cmd
        .spawn()
        .map_err(|e| format!("Failed to locate {tool}: {e}"))?;
    let out = wait_child_output(child, deadline)?;
    if !out.status.success() {
        return Ok(None);
    }
    let raw = decode_command_output(&out.stdout);
    // Cannot blindly take the first line: an interactive .zshrc may print a banner first (e.g. "Welcome back"),
    // with the real path of command -v after it; taking the first line starting with `/` is the stable choice.
    let Some(first) = first_abs_path_line(&raw) else {
        return Ok(None);
    };
    Ok(std::fs::canonicalize(first).ok())
}

#[cfg(target_os = "windows")]
fn windows_path_lookup_command(
    tool: &str,
    effective_path: &std::ffi::OsStr,
) -> std::process::Command {
    use std::os::windows::process::CommandExt;
    use std::process::{Command, Stdio};

    // Use the system copy explicitly so a project-local `where.exe` cannot
    // hijack the passive lookup before the PATH-only pattern is evaluated.
    let where_exe = std::path::PathBuf::from(
        std::env::var_os("SystemRoot").unwrap_or_else(|| std::ffi::OsString::from(r"C:\Windows")),
    )
    .join("System32")
    .join("where.exe");
    let mut command = Command::new(where_exe);
    command
        // `$PATH:pattern` is where.exe's documented environment-variable
        // search form. Unlike a bare pattern, it does not search the current
        // directory before PATH.
        .arg(format!("$PATH:{tool}"))
        .env("PATH", effective_path)
        .creation_flags(CREATE_NO_WINDOW)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    command
}

#[cfg(target_os = "windows")]
fn resolve_path_default(
    tool: &str,
    deadline: Option<CommandDeadline>,
) -> Result<Option<std::path::PathBuf>, String> {
    // Restrict `where` to the merged effective PATH. A bare `where {tool}` also
    // searches the current directory first, which would let a project-local
    // `codex.cmd` be executed by a passive version check. The `$PATH:pattern`
    // form searches only the supplied environment variable while still seeing
    // registry PATH entries lost by an in-app-update relaunch (#6061).
    let current_path = effective_path_os().unwrap_or_default();
    let child = windows_path_lookup_command(tool, &current_path)
        .spawn()
        .map_err(|e| format!("Failed to locate {tool}: {e}"))?;
    let out = wait_child_output(child, deadline)?;
    if !out.status.success() {
        return Ok(None);
    }
    let raw = decode_command_output(&out.stdout);
    // `where` lists every match on PATH in order; the first is what the user
    // actually runs. Skip App Execution Aliases (reparse points under
    // `Microsoft\WindowsApps`) — they launch the Store / a protocol handler,
    // are not CLIs we can `--version`-probe, and must not be treated as the
    // PATH default. Take the first remaining real entry.
    let resolved = raw.lines().map(str::trim).find(|line| {
        !line.is_empty()
            && !is_windows_app_execution_alias_dir(
                Path::new(line).parent().unwrap_or(Path::new("")),
            )
    });
    let Some(first) = resolved else {
        return Ok(None);
    };
    let path = Path::new(first);
    let preferred =
        windows_runnable_sibling_for_extensionless_tool(path).unwrap_or_else(|| path.to_path_buf());
    Ok(std::fs::canonicalize(preferred).ok())
}

/// Per-subprocess probe budget for the upgrade preflight / conflict diagnosis. The enumeration opens one
/// login shell per tool and runs `--version` once per install; any hang (a blocking .zshrc, an nvm shim
/// pointing at a deleted node, ...) would stall the whole "upgrade all" preflight — on timeout the whole group is killed, that probe is degraded to a failure, and the preflight continues.
const INSTALL_PROBE_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(10);

/// `--version` probe with a timeout (non-Windows). Unlike the bare `output()` of `scan_cli_version`,
/// stdin is explicitly null and the child enters its own session via `isolate_child_process_group`, so a timeout can kill the whole group.
/// `new_path` takes `&OsStr` rather than `&str`: the same convention as `prepend_search_dir_to_path`, so
/// non-UTF-8 PATH segments are not lossily dropped in transit.
#[cfg(not(target_os = "windows"))]
fn run_probe_version_command(
    tool_path: &Path,
    new_path: &std::ffi::OsStr,
) -> Result<std::process::Output, String> {
    use std::process::{Command, Stdio};

    let mut cmd = Command::new(tool_path);
    cmd.arg("--version")
        .env("PATH", new_path)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    isolate_child_process_group(&mut cmd);
    let child = cmd.spawn().map_err(|e| e.to_string())?;
    wait_child_output(
        child,
        CommandDeadline::from_timeout(Some(INSTALL_PROBE_TIMEOUT)),
    )
}

/// `--version` probe with a timeout (Windows). The command is built exactly like
/// `run_windows_tool_version_command` (.cmd/.bat via `cmd /C call`, everything else directly), but using
/// spawn + `wait_child_output`: a hung .cmd shim / CLI is killed as a whole tree by `terminate_child_tree`
/// (taskkill /T /F) on timeout, so the preflight is no longer stalled by a single candidate. The scan path keeps the original helper.
#[cfg(target_os = "windows")]
fn run_probe_version_command(
    tool_path: &Path,
    new_path: &str,
) -> Result<std::process::Output, String> {
    use std::process::Stdio;

    let mut cmd = build_windows_tool_command(tool_path, &["--version"], new_path);
    cmd.stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    let child = cmd.spawn().map_err(|e| e.to_string())?;
    wait_child_output(
        child,
        CommandDeadline::from_timeout(Some(INSTALL_PROBE_TIMEOUT)),
    )
}

/// Enumerate every installation of a tool on the system (without short-circuiting). Shares
/// `build_tool_search_paths` with `scan_cli_version`, but does not stop at the first hit — it runs
/// `--version` on every deduplicated real executable, which reveals "the upgrade wrote to A while PATH actually uses B".
pub(crate) fn enumerate_tool_installations(tool: &str) -> Vec<ToolInstallation> {
    let search_paths = build_tool_search_paths(tool);
    #[cfg(target_os = "windows")]
    let current_path = effective_path_string();
    #[cfg(not(target_os = "windows"))]
    let current_path = effective_path_os().unwrap_or_default();
    // A timeout is mandatory: with deadline None, wait_child_output waits forever and a hung login shell
    // would stall the whole preflight permanently (with no feedback in the frontend at this stage). A failed
    // resolution only loses the is_path_default marker, which is not fatal.
    let path_default = resolve_path_default(
        tool,
        CommandDeadline::from_timeout(Some(INSTALL_PROBE_TIMEOUT)),
    )
    .ok()
    .flatten();

    let mut seen: std::collections::HashSet<std::path::PathBuf> = std::collections::HashSet::new();
    let mut installs: Vec<ToolInstallation> = Vec::new();

    for dir in &search_paths {
        #[cfg(target_os = "windows")]
        let new_path = format!("{};{}", dir.display(), current_path);
        #[cfg(not(target_os = "windows"))]
        let new_path = prepend_search_dir_to_path(dir, &current_path);

        for tool_path in tool_executable_candidates(tool, dir) {
            if !tool_path.exists() {
                continue;
            }
            // Deduplicate after resolving symlinks with canonicalize: /opt/homebrew/bin/x -> Cellar/..., nvm shims and
            // other entries may point at the same real file and count as one installation.
            let real = std::fs::canonicalize(&tool_path).unwrap_or_else(|_| tool_path.clone());
            if !seen.insert(real.clone()) {
                continue;
            }

            let output = run_probe_version_command(&tool_path, &new_path);

            let (version, runnable, error) = match output {
                Ok(out) if out.status.success() => {
                    let stdout = decode_command_output(&out.stdout).trim().to_string();
                    let stderr = decode_command_output(&out.stderr).trim().to_string();
                    let raw = if stdout.is_empty() { stderr } else { stdout };
                    (Some(extract_version(&raw)), true, None)
                }
                Ok(out) => {
                    let stderr = decode_command_output(&out.stderr).trim().to_string();
                    let stdout = decode_command_output(&out.stdout).trim().to_string();
                    let detail = if stderr.is_empty() { stdout } else { stderr };
                    let detail = detail.trim();
                    let error = if detail.is_empty() {
                        None
                    } else {
                        Some(last_lines(detail, 4))
                    };
                    (None, false, error)
                }
                Err(e) => (None, false, Some(e)),
            };

            let is_path_default = path_default.as_ref() == Some(&real);
            let path_str = tool_path.display().to_string();
            let source = infer_install_source(&tool_path);

            installs.push(ToolInstallation {
                path: path_str,
                version,
                runnable,
                error,
                source: source.to_string(),
                is_path_default,
                // Reuse the real path already canonicalized around line ~1357 to keep the downstream
                // installs_anchored_command from canonicalizing the same file again.
                real: real.clone(),
            });
        }
    }

    // The PATH default comes first so the UI shows at a glance which install the command line uses.
    installs.sort_by_key(|i| std::cmp::Reverse(i.is_path_default));
    installs
}

/// npm package name of each tool (hermes uses its own CLI/installer and is not listed here). Anchored upgrades build `npm i -g` from it.
/// One table for all platforms — the Windows anchoring layer (the windows version of `anchored_command_from_paths`) reads it too.
// Verbatim upstream official install scripts and npm install commands. The product's own lifecycle spec
// (ADR-0033) compares against these word by word in the drift tests under `compat/ccswitch/lifecycle`,
// so they are kept in test builds only: the product runtime path no longer goes through the upstream command layer.
#[cfg(test)]
pub(crate) const CLAUDE_INSTALL_UNIX: &str =
    "bash -c 'tmp=$(mktemp) && curl -fsSL https://claude.ai/install.sh -o $tmp && bash $tmp; status=$?; rm -f $tmp; exit $status'";
#[cfg(test)]
pub(crate) const OPENCODE_INSTALL_UNIX: &str =
    "bash -c 'tmp=$(mktemp) && curl -fsSL https://opencode.ai/install -o $tmp && bash $tmp; status=$?; rm -f $tmp; exit $status'";
#[cfg(test)]
pub(crate) const GROK_INSTALL_UNIX: &str =
    "bash -c 'tmp=$(mktemp) && curl -fsSL https://x.ai/cli/install.sh -o $tmp && bash $tmp; status=$?; rm -f $tmp; exit $status'";
#[cfg(test)]
pub(crate) const HERMES_INSTALL_UNIX: &str =
    "bash -c 'tmp=$(mktemp) && curl -fsSL https://raw.githubusercontent.com/NousResearch/hermes-agent/main/scripts/install.sh -o $tmp && bash $tmp; status=$?; rm -f $tmp; exit $status'";
#[cfg(test)]
pub(crate) const KIMI_INSTALL_UNIX: &str =
    "bash -c 'tmp=$(mktemp) && curl -fsSL https://code.kimi.com/kimi-code/install.sh -o $tmp && bash $tmp; status=$?; rm -f $tmp; exit $status'";

#[cfg(test)]
pub(crate) fn npm_install_command_for(tool: &str) -> Option<&'static str> {
    match tool {
        "claude" => Some("npm i -g @anthropic-ai/claude-code@latest"),
        "codex" => Some("npm i -g @openai/codex@latest"),
        "gemini" => Some("npm i -g @google/gemini-cli@latest"),
        "grok" => Some("npm i -g @xai-official/grok@latest"),
        "opencode" => Some("npm i -g opencode-ai@latest"),
        "openclaw" => Some("npm i -g openclaw@latest"),
        "pi" => Some("npm i -g @earendil-works/pi-coding-agent@latest"),
        "kimi" => Some("npm i -g @moonshot-ai/kimi-code@latest"),
        "dsh" => Some("npm i -g @deepseek-ai/dsh@latest"),
        _ => None,
    }
}

pub(crate) fn npm_package_for(tool: &str) -> Option<&'static str> {
    match tool {
        "claude" => Some("@anthropic-ai/claude-code"),
        "codex" => Some("@openai/codex"),
        "gemini" => Some("@google/gemini-cli"),
        "grok" => Some("@xai-official/grok"),
        "opencode" => Some("opencode-ai"),
        "openclaw" => Some("openclaw"),
        "pi" => Some("@earendil-works/pi-coding-agent"),
        "kimi" => Some("@moonshot-ai/kimi-code"),
        "dsh" => Some("@deepseek-ai/dsh"),
        _ => None,
    }
}

/// Extract the Homebrew formula name from a canonicalized real path:
/// `/opt/homebrew/Cellar/gemini-cli/0.13.0/...` → `Some("gemini-cli")`。
/// A non-Cellar path (= not a formula, possibly an npm global package installed by Homebrew node) returns None.
/// The key distinction: even when a formula uses node internally, its real path is under `Cellar/<formula>/`, while a Homebrew
/// npm global package lands in `/opt/homebrew/lib/node_modules` (no Cellar). The two need different upgrade commands.
#[cfg(not(target_os = "windows"))]
pub(crate) fn brew_formula_from_path(real: &str) -> Option<String> {
    let mut segs = real.split('/');
    while let Some(seg) = segs.next() {
        if seg.eq_ignore_ascii_case("Cellar") {
            return segs.next().filter(|s| !s.is_empty()).map(|s| s.to_string());
        }
    }
    None
}

/// An anchored path that goes through a `.bat` file and is **invoked by `call`** needs two layers of
/// defence against batch special characters:
///
/// **(1) `%` goes through two rounds of percent expansion -> escape with four `%`**. The standard escape
/// for a literal `%` in a .bat is `%%`, but the `call` command (Microsoft `call /?`: "percent (%) expansion is
/// performed on each parameter") **runs another round after the batch parser has turned `%%` into `%`**.
/// So `%%FOO%%` in the source .bat becomes `%FOO%` after the first round, and the second round of call
/// treats it as a variable reference and expands it again — to make call see the literal `%FOO%` you must
/// write `%%%%FOO%%%%` (first round -> `%%FOO%%`, second round -> literal `%FOO%`). This is the one character
/// **quotes cannot protect** in cmd: a `%` inside quotes still goes through both rounds.
///
/// **(2) Token boundary / escape characters trigger outer double quotes**: any of `' '` `'&'` `'('` `')'` `'^'`
/// `';'` `'<'` `'>'` `'|'` `','` forces quoting. NTFS allows these characters in paths, and without quotes cmd
/// would split the path into several tokens while `^` would trigger an escape; inside quotes they are literal,
/// and the second parse by call does not treat them specially either (`^` loses its escape meaning inside
/// quotes, and token boundary characters are literal inside quotes).
///
/// `!` (delayed expansion) only takes effect under `setlocal enabledelayedexpansion`, which our .bat header
/// (`@echo off` only) does not enable, so it needs no handling. `'` has no special meaning in cmd.
///
/// Mirrors the "lightweight conditional wrapper" semantics of the POSIX `quote_path_if_spaced`: a path with
/// no special characters stays bare (cleaner command display), otherwise it is wrapped by `win_double_quote` with the necessary escaping.
#[cfg(target_os = "windows")]
fn win_quote_path_for_batch(p: &str) -> String {
    // `%` goes through two rounds of expansion: one in the .bat parser plus one in `call` (Microsoft `call /?`:
    // "percent (%) expansion is performed on each parameter"). To make call finally see a literal `%`,
    // four are needed -> `%%%%` (batch round -> `%%`, call round -> literal `%`).
    // Quoted text still goes through both rounds, so this step is independent of the outer quotes and unconditional.
    let escaped = if p.contains('%') {
        p.replace('%', "%%%%")
    } else {
        p.to_string()
    };
    // Note: `needs_quote` is decided from the **original path** `p`, not from `escaped` — the `%` characters
    // the latter introduces are not "special trigger characters", otherwise a path containing `%` would be wrongly quoted.
    let needs_quote = p
        .chars()
        .any(|c| matches!(c, ' ' | '&' | '(' | ')' | '^' | ';' | '<' | '>' | '|' | ','));
    if needs_quote {
        win_double_quote(&escaped)
    } else {
        escaped
    }
}

/// Which tools prefer their "official self-update" over a package manager upgrade (producing `<tool> update || <pkg-mgr>`).
///
/// **codex is deliberately excluded**: on an npm install `codex update` is just a bare `npm install -g
/// @openai/codex` (no `@latest` / `--include=optional`, no uninstall first), yet it only checks the exit code
/// and unconditionally prints "Update ran successfully". When npm fails to install the platform binary optional
/// dependency `@openai/codex-<triple>` it still **exits 0 with a false success**, short-circuiting the outer `||`
/// fallback and hiding the breakage behind a success toast (the user-reported "Missing optional dependency"
/// comes from exactly this). So codex always uses the npm anchored upgrade; when it is genuinely broken
/// (`runnable=false`), the gate in `installs_anchored_command` switches to the uninstall+install self-repair of `codex_repair_command` instead of codex's own self-update.
pub(crate) fn prefers_official_update(tool: &str, shell: LifecycleCommandShell) -> bool {
    match shell {
        LifecycleCommandShell::Posix => {
            matches!(tool, "claude" | "opencode" | "openclaw")
        }
        LifecycleCommandShell::WindowsBatch => {
            matches!(
                tool,
                // Before anomalyco/opencode#17295 was fixed, the Windows `upgrade` of OpenCode could show an
                // interactive prompt when install-method detection failed (it spawned npm.cmd without shell:true);
                // the silent lifecycle has no stdin and would hang, so Windows anchors to the package manager
                // path first and opencode can be added back here once upstream fixes it.
                "claude" | "openclaw"
            )
        }
    }
}

/// Pick "the install the command line actually hits" from the enumeration: prefer `is_path_default`;
/// otherwise (the PATH default could not be resolved but there is exactly one install) take that one;
/// several installs with no default marker -> None (nothing to anchor to).
///
/// Shared by all platforms — both the POSIX and Windows `anchored_command_from_paths` call it through
/// `installs_anchored_command` to take the default install and canonicalize its real path.
pub(crate) fn default_install(installs: &[ToolInstallation]) -> Option<&ToolInstallation> {
    installs.iter().find(|i| i.is_path_default).or_else(|| {
        if installs.len() == 1 {
            installs.first()
        } else {
            None
        }
    })
}

#[derive(Clone, Copy)]
struct CommandDeadline {
    expires_at: std::time::Instant,
    limit: std::time::Duration,
}

impl CommandDeadline {
    fn from_timeout(timeout: Option<std::time::Duration>) -> Option<Self> {
        timeout.map(|limit| Self {
            expires_at: std::time::Instant::now() + limit,
            limit,
        })
    }

    fn remaining(self) -> Result<std::time::Duration, String> {
        self.expires_at
            .checked_duration_since(std::time::Instant::now())
            .filter(|remaining| !remaining.is_zero())
            .ok_or_else(|| self.timeout_error())
    }

    fn timeout_error(self) -> String {
        format!("Command timed out after {}s", self.limit.as_secs())
    }
}

#[cfg(target_os = "windows")]
fn terminate_child_tree(child: &mut std::process::Child) -> bool {
    use std::os::windows::process::CommandExt;
    use std::process::{Command, Stdio};

    let status = Command::new("taskkill")
        .args(["/PID", &child.id().to_string(), "/T", "/F"])
        .creation_flags(CREATE_NO_WINDOW)
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status();
    matches!(status, Ok(status) if status.success()) || child.kill().is_ok()
}

#[cfg(not(target_os = "windows"))]
fn terminate_child_tree(child: &mut std::process::Child) -> bool {
    let process_group = -(child.id() as libc::pid_t);
    // SAFETY: runtime commands are placed in a dedicated process group before spawn.
    (unsafe { libc::kill(process_group, libc::SIGKILL) == 0 }) || child.kill().is_ok()
}

#[cfg(not(target_os = "windows"))]
fn isolate_child_process_group(cmd: &mut std::process::Command) {
    use std::os::unix::process::CommandExt;

    // setsid rather than process_group(0): a new session comes with a new process group (leader = self, so
    // the kill(-pid) whole-group semantics of terminate_child_tree are unchanged) and additionally **detaches
    // the controlling terminal**. With only process group isolation, the interactive shell used for probing
    // (zsh -lic) would still hold the controlling terminal (e.g. in dev mode started from a terminal) and its
    // job control would stop it with SIGTTIN/SIGTTOU in a background process group, so `wait()` would never
    // return. After detaching, the shell cannot get /dev/tty and job control turns itself off.
    // SAFETY: setsid is async-signal-safe, and the forked child inherits the parent process group so it is
    // never the group leader; the call cannot fail with EPERM.
    unsafe {
        cmd.pre_exec(|| {
            if libc::setsid() == -1 {
                return Err(std::io::Error::last_os_error());
            }
            Ok(())
        });
    }
}

fn wait_child_output(
    mut child: std::process::Child,
    deadline: Option<CommandDeadline>,
) -> Result<std::process::Output, String> {
    use std::io::Read;

    let stdout_pipe = child.stdout.take();
    let stderr_pipe = child.stderr.take();

    let stdout_handle = stdout_pipe.map(|mut pipe| {
        std::thread::spawn(move || {
            let mut buf = Vec::new();
            let _ = pipe.read_to_end(&mut buf);
            buf
        })
    });
    let stderr_handle = stderr_pipe.map(|mut pipe| {
        std::thread::spawn(move || {
            let mut buf = Vec::new();
            let _ = pipe.read_to_end(&mut buf);
            buf
        })
    });

    let status = match deadline {
        None => child
            .wait()
            .map_err(|e| format!("Failed to wait for command: {e}"))?,
        Some(deadline) => {
            loop {
                match child.try_wait() {
                    Ok(Some(status)) => break status,
                    Ok(None) => {
                        let remaining = match deadline.remaining() {
                            Ok(remaining) => remaining,
                            Err(error) => {
                                if terminate_child_tree(&mut child) {
                                    let _ = child.wait();
                                }
                                // Do not join pipe readers on timeout. If tree termination fails,
                                // a descendant may still own the write handle and never produce EOF.
                                drop(stdout_handle);
                                drop(stderr_handle);
                                return Err(error);
                            }
                        };
                        std::thread::sleep(std::cmp::min(
                            std::time::Duration::from_millis(50),
                            remaining,
                        ));
                    }
                    Err(e) => {
                        if terminate_child_tree(&mut child) {
                            let _ = child.wait();
                        }
                        return Err(format!("Failed to wait for command: {e}"));
                    }
                }
            }
        }
    };

    if let Some(deadline) = deadline {
        while stdout_handle
            .as_ref()
            .is_some_and(|handle| !handle.is_finished())
            || stderr_handle
                .as_ref()
                .is_some_and(|handle| !handle.is_finished())
        {
            let remaining = match deadline.remaining() {
                Ok(remaining) => remaining,
                Err(error) => {
                    let _ = terminate_child_tree(&mut child);
                    drop(stdout_handle);
                    drop(stderr_handle);
                    return Err(error);
                }
            };
            std::thread::sleep(std::cmp::min(
                std::time::Duration::from_millis(50),
                remaining,
            ));
        }
    }

    let stdout = stdout_handle
        .map(|handle| handle.join().unwrap_or_default())
        .unwrap_or_default();
    let stderr = stderr_handle
        .map(|handle| handle.join().unwrap_or_default())
        .unwrap_or_default();

    Ok(std::process::Output {
        status,
        stdout,
        stderr,
    })
}

#[cfg(target_os = "windows")]
pub(crate) fn wsl_distro_for_tool(tool: &str) -> Option<String> {
    let override_dir = match tool {
        "claude" => crate::settings::get_claude_override_dir(),
        "codex" => crate::settings::get_codex_override_dir(),
        "gemini" => crate::settings::get_gemini_override_dir(),
        "grok" => crate::settings::get_grok_override_dir(),
        "opencode" => crate::settings::get_opencode_override_dir(),
        "openclaw" => crate::settings::get_openclaw_override_dir(),
        "hermes" => crate::settings::get_hermes_override_dir(),
        "pi" => crate::settings::get_pi_override_dir(),
        "kimi" | "dsh" => None,
        _ => None,
    }?;

    wsl_distro_from_path(&override_dir)
}

#[cfg(not(target_os = "windows"))]
pub(crate) fn wsl_distro_for_tool(_tool: &str) -> Option<String> {
    None
}

/// Extract the WSL distribution name from a UNC path
/// Supports both `\\wsl$\Ubuntu\...` and `\\wsl.localhost\Ubuntu\...`
#[cfg(target_os = "windows")]
fn wsl_distro_from_path(path: &Path) -> Option<String> {
    use std::path::{Component, Prefix};
    let Some(Component::Prefix(prefix)) = path.components().next() else {
        return None;
    };
    match prefix.kind() {
        Prefix::UNC(server, share) | Prefix::VerbatimUNC(server, share) => {
            let server_name = server.to_string_lossy();
            if server_name.eq_ignore_ascii_case("wsl$")
                || server_name.eq_ignore_ascii_case("wsl.localhost")
            {
                let distro = share.to_string_lossy().to_string();
                if !distro.is_empty() {
                    return Some(distro);
                }
            }
            None
        }
        _ => None,
    }
}

/// Set the window theme (Windows/macOS title bar colour)
/// theme: "dark" | "light" | "system"
#[tauri::command]
pub async fn set_window_theme(window: tauri::Window, theme: String) -> Result<(), String> {
    use tauri::Theme;

    let tauri_theme = match theme.as_str() {
        "dark" => Some(Theme::Dark),
        "light" => Some(Theme::Light),
        _ => None, // system default
    };

    window.set_theme(tauri_theme).map_err(|e| e.to_string())?;

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::{Path, PathBuf};

    /// Happy path of the probe helper: spawn (including pre_exec setsid) starts and the output is captured.
    /// `/bin/echo --version` exits successfully immediately on both macOS and Linux.
    #[cfg(not(target_os = "windows"))]
    #[test]
    fn probe_version_command_captures_healthy_tool_output() {
        let out = run_probe_version_command(Path::new("/bin/echo"), std::ffi::OsStr::new(""))
            .expect("probe of /bin/echo should succeed");
        assert!(out.status.success());
    }

    /// Timeout kill path: a hung child is killed as a whole group on time and wait returns a timeout error instead of waiting forever.
    /// Also pins the semantics after the setsid change — the child is the leader of a new session/process group,
    /// so the kill(-pid) of terminate_child_tree still reaches it (regression line: reverting to process_group
    /// or dropping the isolation breaks the kill path of this test).
    #[cfg(not(target_os = "windows"))]
    #[test]
    fn isolated_hung_child_is_killed_on_deadline() {
        use std::process::{Command, Stdio};

        let mut cmd = Command::new("/bin/sh");
        cmd.args(["-c", "sleep 30"])
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        isolate_child_process_group(&mut cmd);
        let child = cmd.spawn().expect("spawn sleep");
        let started = std::time::Instant::now();
        let result = wait_child_output(
            child,
            CommandDeadline::from_timeout(Some(std::time::Duration::from_millis(200))),
        );
        assert!(result.is_err(), "expected timeout error, got {result:?}");
        assert!(
            started.elapsed() < std::time::Duration::from_secs(5),
            "kill should return promptly instead of waiting out the sleep"
        );
    }

    #[test]
    fn test_extract_version() {
        assert_eq!(extract_version("claude 1.0.20"), "1.0.20");
        assert_eq!(extract_version("v2.3.4-beta.1"), "2.3.4-beta.1");
        assert_eq!(extract_version("no version here"), "no version here");
    }

    #[test]
    fn github_release_version_prefers_semver_in_name_over_calendar_tag() {
        // Official Hermes release: the tag is calendar-based and the semantic version only appears in the name; since 2026-08-19 there is an extra v inside the parentheses
        for (name, tag, want) in [
            ("Hermes Agent v0.20.4 (2026.8.18)", "v2026.8.18", "0.20.4"),
            ("Hermes Agent v0.21.0 (v2026.8.31)", "v2026.8.31", "0.21.0"),
            (
                "Hermes Agent v0.20.3 (2026.8.16.2)",
                "v2026.8.16.2",
                "0.20.3",
            ),
        ] {
            let json = serde_json::json!({ "name": name, "tag_name": tag });
            assert_eq!(
                github_release_version_from_json(&json).as_deref(),
                Some(want),
                "{name}"
            );
        }
    }

    #[test]
    fn github_release_version_falls_back_to_semver_tag() {
        // opencode: name == tag, so taking name or tag gives the same result
        let named = serde_json::json!({ "name": "v1.18.18", "tag_name": "v1.18.18" });
        assert_eq!(
            github_release_version_from_json(&named).as_deref(),
            Some("1.18.18")
        );
        let prose = serde_json::json!({ "name": "August refresh", "tag_name": "v1.18.18" });
        assert_eq!(
            github_release_version_from_json(&prose).as_deref(),
            Some("1.18.18")
        );
        let unnamed = serde_json::json!({ "tag_name": "v1.2.3" });
        assert_eq!(
            github_release_version_from_json(&unnamed).as_deref(),
            Some("1.2.3")
        );
    }

    #[test]
    fn github_release_version_rejects_calendar_versions_and_rate_limit_body() {
        // Both name and tag are calendar-style numbers only: 2026.8.31 must not be treated as a version (the frontend would permanently claim "update available")
        let calendar = serde_json::json!({
            "name": "Hermes Agent (2026.8.31)",
            "tag_name": "v2026.8.31"
        });
        assert_eq!(github_release_version_from_json(&calendar), None);
        let four_seg = serde_json::json!({ "tag_name": "v2026.8.16.2" });
        assert_eq!(github_release_version_from_json(&four_seg), None);
        // The unauthenticated GitHub rate limit response only has message / documentation_url
        let limited = serde_json::json!({
            "message": "API rate limit exceeded for 1.2.3.4.",
            "documentation_url": "https://docs.github.com/rest"
        });
        assert_eq!(github_release_version_from_json(&limited), None);
        assert_eq!(
            github_release_version_from_json(&serde_json::json!({})),
            None
        );
    }

    #[test]
    fn hermes_pypi_fallback_is_hidden_when_local_leads() {
        let pypi = || Some("0.19.0".to_string());
        // GitHub unreachable and PyPI still at 0.19.0: with local 0.21.0 the stale value is not shown (otherwise "latest < current")
        assert_eq!(drop_latest_behind_local(pypi(), Some("0.21.0")), None);
        // Local equal to / behind PyPI, or local unknown: display as usual
        assert_eq!(
            drop_latest_behind_local(pypi(), Some("0.19.0")).as_deref(),
            Some("0.19.0")
        );
        assert_eq!(
            drop_latest_behind_local(pypi(), Some("0.18.2")).as_deref(),
            Some("0.19.0")
        );
        assert_eq!(
            drop_latest_behind_local(pypi(), None).as_deref(),
            Some("0.19.0")
        );
        // When the local version cannot be parsed, conservatively treat it as not leading
        assert_eq!(
            drop_latest_behind_local(pypi(), Some("unknown")).as_deref(),
            Some("0.19.0")
        );
        assert_eq!(drop_latest_behind_local(None, Some("0.21.0")), None);
    }

    #[test]
    fn test_compare_semver() {
        use std::cmp::Ordering;
        assert_eq!(
            compare_semver("2.1.156", "2.1.154"),
            Some(Ordering::Greater)
        );
        assert_eq!(compare_semver("2.1.154", "2.1.156"), Some(Ordering::Less));
        assert_eq!(compare_semver("2.1.156", "2.1.156"), Some(Ordering::Equal));
        // A prerelease is lower than the release with the same core
        assert_eq!(
            compare_semver("2.1.156-beta.1", "2.1.156"),
            Some(Ordering::Less)
        );
        // A prerelease with a higher core still beats a lower release (the gemini nightly case)
        assert_eq!(
            compare_semver("0.45.0-nightly.1", "0.44.1"),
            Some(Ordering::Greater)
        );
        // A large patch (codex timestamp style) does not overflow
        assert_eq!(
            compare_semver("0.1.2505172116", "0.135.0"),
            Some(Ordering::Less)
        );
        // Unparseable returns None (the dirty gemini `false` tag)
        assert_eq!(compare_semver("false", "1.0.0"), None);
    }

    #[test]
    fn test_pick_latest_version() {
        use serde_json::json;
        let tags = json!({
            "latest": "2.1.154",
            "next": "2.1.156",
            "stable": "2.1.145"
        });
        let map = tags.as_object().unwrap();

        // Local leads latest (on the next channel) -> next is queried and the numbers line up
        assert_eq!(
            pick_latest_version(map, &["next"], Some("2.1.156"), None),
            Some("2.1.156".to_string())
        );
        // Local equals latest -> no extra query, latest is still shown
        assert_eq!(
            pick_latest_version(map, &["next"], Some("2.1.154"), None),
            Some("2.1.154".to_string())
        );
        // Local behind latest (stable channel user) -> no extra query, never pushed to a prerelease
        assert_eq!(
            pick_latest_version(map, &["next"], Some("2.1.145"), None),
            Some("2.1.154".to_string())
        );
        // No prerelease allowlist -> always latest only (local is not even parsed, so a dirty local cannot trigger anything)
        assert_eq!(
            pick_latest_version(map, &[], Some("2.1.156"), None),
            Some("2.1.154".to_string())
        );
        // Local version unknown -> conservatively latest only
        assert_eq!(
            pick_latest_version(map, &["next"], None, None),
            Some("2.1.154".to_string())
        );
    }

    /// Looking up one version number must not download 13 MB: the dist-tags endpoint is the only URL built here,
    /// and the `/` in a scoped name must be preserved verbatim (the registry accepts it as a path; encoded it 404s).
    #[test]
    fn npm_dist_tags_url_keeps_scoped_names_verbatim() {
        assert_eq!(
            npm_dist_tags_url("@openai/codex"),
            "https://registry.npmjs.org/-/package/@openai/codex/dist-tags"
        );
        assert_eq!(
            npm_dist_tags_url("opencode-ai"),
            "https://registry.npmjs.org/-/package/opencode-ai/dist-tags"
        );
    }

    #[test]
    fn test_pick_latest_version_uses_newer_published_stable_when_requested() {
        use serde_json::json;
        let metadata = json!({
            "dist-tags": {"latest": "0.1.4", "alpha": "1.0.13"},
            "versions": {"0.1.4": {}, "1.0.12": {}, "1.0.13": {}, "1.1.0-beta.1": {}}
        });
        let tags = metadata.get("dist-tags").unwrap().as_object().unwrap();
        let versions = metadata.get("versions").unwrap().as_object().unwrap();

        assert_eq!(
            pick_latest_version(tags, &[], Some("0.1.4"), Some(versions)),
            Some("1.0.13".to_string())
        );
    }

    #[test]
    fn test_pick_latest_version_filters_dirty_prerelease() {
        use serde_json::json;
        // Simulating codex: beta is a timestamp-style dirty version below latest
        let tags = json!({
            "latest": "0.135.0",
            "beta": "0.1.2505172116"
        });
        let map = tags.as_object().unwrap();
        // Even when local leads latest, a dirty beta below latest is not selected
        assert_eq!(
            pick_latest_version(map, &["beta"], Some("0.200.0"), None),
            Some("0.135.0".to_string())
        );
    }

    /// `parent_dir` is the foundation of the anchoring layer's "derive a same-directory absolute path from the bin path"
    /// and is shared across platforms — this pins the four cases `\`, `/`, mixed separators and the root boundary so a future refactor cannot silently change the semantics.
    mod parent_dir_cases {}

    /// Windows-only anchored upgrade regression (equivalence classes compressed to 3 idioms: volta/pnpm/npm).
    /// The whole block is gated by `cfg(target_os = "windows")` and does not participate in cargo test on
    /// macOS/Linux; Windows CI runs the full set. A tempdir simulates the sibling entry existing or not, pinning
    /// three things: extension order priority, automatic double quoting for paths with spaces, and "no sibling found -> None -> static fallback".
    #[cfg(target_os = "windows")]
    mod anchored_upgrade_windows {
        use super::super::*;

        /// Create a subdirectory `subdir` under the tempdir (the tempdir root when empty), placing `entry`
        /// and some fake `siblings` in it. Returns `(TempDir, subdirectory, absolute entry path)` — the TempDir
        /// must be kept alive, otherwise the fs files vanish on drop, `is_file()` fails and the test goes falsely green.
        fn setup_sibling(
            subdir: &str,
            entry: &str,
            siblings: &[&str],
        ) -> (tempfile::TempDir, std::path::PathBuf, String) {
            let dir = tempfile::tempdir().unwrap();
            let sub = if subdir.is_empty() {
                dir.path().to_path_buf()
            } else {
                dir.path().join(subdir)
            };
            std::fs::create_dir_all(&sub).unwrap();
            std::fs::write(sub.join(entry), "").unwrap();
            for s in siblings {
                std::fs::write(sub.join(s), "").unwrap();
            }
            let bin_path = sub.join(entry).to_string_lossy().to_string();
            (dir, sub, bin_path)
        }

        /// **Must stay a mirror of the body of `win_quote_path_for_batch`** — it computes the expected value for the
        /// anchored tests dynamically so they also pass on dev machines whose temp root contains spaces / `&` / `(` /
        /// `%` and similar (the default Windows `%TEMP%` is `C:\Users\<user>\AppData\Local\Temp`, so on a machine
        /// with a space in the user name the whole path contains a space: production code quotes it correctly while a
        /// hardcoded unquoted expected value would fail falsely).
        ///
        /// The mirror introduces an implicit "these two must stay in sync" dependency — the regression net is the seven
        /// standalone unit tests of `win_quote_*`, which pin the quoting rules themselves with hardcoded literals, so
        /// even if this mirror drifts that group catches it, and vice versa.
        fn expect_quoted_path(p: &str) -> String {
            let escaped = p.replace('%', "%%%%");
            let needs_quote = p
                .chars()
                .any(|c| matches!(c, ' ' | '&' | '(' | ')' | '^' | ';' | '<' | '>' | '|' | ','));
            if needs_quote {
                format!("\"{escaped}\"")
            } else {
                escaped
            }
        }
    }

    /// Unit tests for Windows-only helpers — the whole block is excluded by cfg on macOS/Linux and does not
    /// participate in `cargo test` there. Windows CI (or cargo test on a Windows machine) activates them. Covers:
    /// (1) double-quote quoting mirroring the POSIX version; (2) sibling_bin_with_ext finding the first existing
    /// extension in order on the fs and returning None when none exist or the dir is empty. tempdir provides a clean fs sandbox.
    #[cfg(target_os = "windows")]
    mod windows_helpers {
        use super::super::*;

        #[test]
        fn win_quote_clean_path_stays_bare() {
            // A plain path with no special characters -> no quotes, so the command displays cleanly.
            assert_eq!(
                win_quote_path_for_batch("C:\\Users\\me\\npm.cmd"),
                "C:\\Users\\me\\npm.cmd"
            );
        }

        #[test]
        fn win_quote_spaced_path_gets_quoted() {
            assert_eq!(
                win_quote_path_for_batch("C:\\Program Files\\nodejs\\npm.cmd"),
                "\"C:\\Program Files\\nodejs\\npm.cmd\""
            );
        }

        #[test]
        fn win_quote_ampersand_path_gets_quoted() {
            // `&` is the cmd command separator and NTFS allows it in paths; without quotes `call C:\A&B\npm.cmd`
            // would be parsed as the two commands `call C:\A` and `B\npm.cmd` and execution would go wrong.
            assert_eq!(
                win_quote_path_for_batch("C:\\Tools&Dev\\npm.cmd"),
                "\"C:\\Tools&Dev\\npm.cmd\""
            );
        }

        #[test]
        fn win_quote_parens_path_gets_quoted() {
            // `(` / `)` mean code blocks in a .bat and are only literal inside quotes.
            assert_eq!(
                win_quote_path_for_batch("C:\\Foo(x86)\\npm.cmd"),
                "\"C:\\Foo(x86)\\npm.cmd\""
            );
        }

        #[test]
        fn win_quote_caret_path_gets_quoted() {
            // `^` is the cmd escape character; wrapped in quotes it is literal.
            assert_eq!(
                win_quote_path_for_batch("C:\\foo^bar\\npm.cmd"),
                "\"C:\\foo^bar\\npm.cmd\""
            );
        }

        #[test]
        fn win_quote_percent_is_escaped_to_quadruple_percent() {
            // `%` goes through one .bat round plus one call round of expansion; to make call finally see the literal
            // `%FOO%`, the source .bat must contain `%%%%FOO%%%%` (first round -> `%%FOO%%`, second -> literal `%FOO%`).
            // The twofold `%%` escape is only correct for echo / direct execution; on a call invocation it is restored
            // to a variable reference and substituted. **This case pins that the "call second parse" must be closed by the fourfold escape**.
            assert_eq!(
                win_quote_path_for_batch("C:\\path%foo%\\npm.cmd"),
                "C:\\path%%%%foo%%%%\\npm.cmd"
            );
        }

        #[test]
        fn win_quote_percent_with_space_gets_both() {
            // The fourfold `%` escape is orthogonal to the outer quotes — a space triggers quoting, a `%` triggers the `%%%%` escape, and they stack.
            assert_eq!(
                win_quote_path_for_batch("C:\\my %dir%\\npm.cmd"),
                "\"C:\\my %%%%dir%%%%\\npm.cmd\""
            );
        }

        #[test]
        fn win_quote_needs_quote_uses_original_path() {
            // Regression guard: `needs_quote` is decided from the **original path**, not from the escaped string —
            // otherwise a path with no token boundary characters (e.g. `C:\path%foo%\npm.cmd`) would be wrongly
            // classified as "needs quoting" after the escape introduces more `%`. This is a hidden entry point for implementation bugs.
            // The input has no token boundary characters -> no outer quotes, only the fourfold `%` escape.
            let out = win_quote_path_for_batch("C:\\foo%bar%\\npm.cmd");
            assert!(
                !out.starts_with('"'),
                "a pure `%` path must not get outer quotes: {out}"
            );
        }
    }

    /// `infer_install_source` is the entry point for deciding the anchoring idiom — nvm/homebrew/volta/pnpm/...
    /// each map to a different upgrade command shape. The function already normalizes with
    /// `replace('\\','/').to_ascii_lowercase()`, so Windows backslashes and case differences need no platform split
    /// here. These assertions pin which path counts as which source so reordering the substrings later cannot silently change the classification.
    mod install_source_classification {
        use super::super::*;
        use std::path::Path;

        #[test]
        fn macos_volta_with_dot_prefix() {
            assert_eq!(
                infer_install_source(Path::new("/Users/me/.volta/bin/codex")),
                "volta"
            );
        }

        #[test]
        fn windows_volta_localappdata_no_dot() {
            // `%LOCALAPPDATA%\Volta\bin\codex.exe` — no leading dot, matched by the `/volta/` fallback
            // (lowercased after normalization). Recognizing only `/.volta/` would drop this Windows case into system.
            assert_eq!(
                infer_install_source(Path::new(
                    "C:\\Users\\me\\AppData\\Local\\Volta\\bin\\codex.exe"
                )),
                "volta"
            );
        }

        #[test]
        fn windows_pnpm_localappdata() {
            // `%LOCALAPPDATA%\pnpm\codex.cmd` — the pnpm global bin directory; recognized as pnpm, the anchored
            // command uses `pnpm add -g <pkg>@latest` rather than the sibling npm.
            assert_eq!(
                infer_install_source(Path::new("C:\\Users\\me\\AppData\\Local\\pnpm\\codex.cmd")),
                "pnpm"
            );
        }

        #[test]
        fn windows_nvm_falls_back_to_system() {
            // Tool paths installed by nvm-windows do not contain `.nvm` (it usually installs under `%APPDATA%\nvm` or
            // the `C:\Program Files\nodejs` symlink), and it is deliberately not recognized as a dedicated source —
            // the anchoring layer treats it as system -> sibling npm.cmd, matching the real nvm-windows idiom
            // (its global packages are installed by the npm of the currently selected node).
            assert_eq!(
                infer_install_source(Path::new(
                    "C:\\Users\\me\\AppData\\Roaming\\nvm\\v22.0.0\\codex.cmd"
                )),
                "system"
            );
        }

        #[test]
        fn windows_scoop_still_identified() {
            // A `/scoop/` branch already exists; none of our six tools is a scoop formula, so this does not actually
            // affect anchoring decisions (the anchoring layer uses the sibling npm.cmd), but the classification is kept for the future.
            assert_eq!(
                infer_install_source(Path::new("C:\\Users\\me\\scoop\\shims\\codex.cmd")),
                "scoop"
            );
        }
    }

    /// Anchored upgrade command generation: real surveyed install paths pinned as regression assertions —
    /// on one machine four tools happen to map to four upgrade methods (native self-update / brew / nvm npm /
    /// homebrew npm), and any change that breaks one of them is caught immediately by these cases.
    #[cfg(not(target_os = "windows"))]
    mod anchored_upgrade {
        use super::super::*;
        use std::path::Path;

        fn inst(path: &str, is_default: bool) -> ToolInstallation {
            ToolInstallation {
                path: path.to_string(),
                version: None,
                runnable: true,
                error: None,
                source: infer_install_source(Path::new(path)).to_string(),
                is_path_default: is_default,
                // Tests do not need the fs canonicalize — the POSIX anchoring tests care about path/real both being
                // passed to the pure string logic of anchored_command_from_paths. The existing cases
                // (brew_formula_extraction / claude_native_*) call anchored_command_from_paths directly rather than
                // going through installs_anchored_command, so here real only feeds the upper default_install + read
                // and the same value suffices.
                real: std::path::PathBuf::from(path),
            }
        }

        #[test]
        fn brew_formula_extraction() {
            assert_eq!(
                brew_formula_from_path("/opt/homebrew/Cellar/gemini-cli/0.13.0/bin/gemini")
                    .as_deref(),
                Some("gemini-cli")
            );
            // A node global package is not under Cellar -> not a formula.
            assert_eq!(
                brew_formula_from_path("/opt/homebrew/lib/node_modules/openclaw/openclaw.mjs"),
                None
            );
            assert_eq!(
                brew_formula_from_path("/Users/me/.nvm/versions/node/v22/lib/node_modules/x"),
                None
            );
        }

        #[test]
        fn default_install_prefers_path_default() {
            let installs = vec![
                inst("/opt/homebrew/bin/openclaw", false),
                inst("/Users/me/.nvm/versions/node/v22/bin/openclaw", true),
            ];
            assert_eq!(
                default_install(&installs).map(|i| i.path.as_str()),
                Some("/Users/me/.nvm/versions/node/v22/bin/openclaw")
            );
        }

        #[test]
        fn default_install_falls_back_to_sole_entry() {
            let installs = vec![inst("/opt/homebrew/bin/gemini", false)];
            assert_eq!(
                default_install(&installs).map(|i| i.path.as_str()),
                Some("/opt/homebrew/bin/gemini")
            );
        }

        #[test]
        fn default_install_none_when_ambiguous() {
            let installs = vec![
                inst("/opt/homebrew/bin/openclaw", false),
                inst("/Users/me/.nvm/versions/node/v22/bin/openclaw", false),
            ];
            assert!(default_install(&installs).is_none());
        }

        #[test]
        fn first_abs_path_line_skips_shell_noise() {
            // An interactive .zshrc prints a banner first (e.g. powerlevel10k / a custom prompt),
            // with the real path of command -v after it -> skip the noise and take the real path.
            assert_eq!(
                first_abs_path_line("🚀 Welcome back!\n/Users/me/.local/bin/claude\n"),
                Some("/Users/me/.local/bin/claude")
            );
            // Without noise, take the first line.
            assert_eq!(
                first_abs_path_line("/opt/homebrew/bin/gemini\n"),
                Some("/opt/homebrew/bin/gemini")
            );
            // No absolute path anywhere in the output -> None.
            assert_eq!(first_abs_path_line("welcome\nbye\n"), None);
        }

        #[test]
        fn path_line_from_env_output_survives_shell_noise() {
            // The stdout of `$SHELL -lic /usr/bin/env` may be preceded by the banner of an interactive rc.
            let raw = "🚀 Welcome back, Jason!\nSHELL=/bin/zsh\nPATH=/opt/homebrew/bin:/usr/bin\nHOME=/Users/me\n";
            assert_eq!(
                path_line_from_env_output(raw),
                Some("/opt/homebrew/bin:/usr/bin")
            );
            // When a multi-line environment variable happens to have a line starting with `PATH=`, the "value must start with /" rule filters it out.
            let poisoned = "SOME_SCRIPT=line1\nPATH=not-a-path\nPATH=/usr/bin:/bin\n";
            assert_eq!(path_line_from_env_output(poisoned), Some("/usr/bin:/bin"));
            // No PATH line at all -> None, and the caller keeps injecting nothing.
            assert_eq!(path_line_from_env_output("HOME=/Users/me\n"), None);
        }

        #[test]
        fn merge_path_segments_dedupes_preserving_login_order() {
            // All login shell segments come first and keep their order; new segments from the inherited PATH are appended.
            assert_eq!(
                merge_path_segments(
                    "/Users/me/.nvm/versions/node/v22/bin:/usr/bin:/bin",
                    "/usr/bin:/bin:/usr/sbin"
                ),
                "/Users/me/.nvm/versions/node/v22/bin:/usr/bin:/bin:/usr/sbin"
            );
            // Empty segments (`a::b` means the current directory in POSIX) must not be injected.
            assert_eq!(merge_path_segments("/usr/bin::/bin", ""), "/usr/bin:/bin");
        }
    }

    /// The "upstream recommendation || npm fallback" short-circuit chain on the install side: the upstream fact of
    /// tool -> official install method pinned as regression assertions. Any change that breaks the chain structure or a URL is caught by these cases.
    #[cfg(not(target_os = "windows"))]
    mod install_strategy {}

    #[cfg(target_os = "windows")]
    mod wsl_helpers {
        use super::super::*;

        #[test]
        fn test_is_valid_shell() {
            assert!(is_valid_shell("bash"));
            assert!(is_valid_shell("zsh"));
            assert!(is_valid_shell("sh"));
            assert!(is_valid_shell("fish"));
            assert!(is_valid_shell("dash"));
            assert!(is_valid_shell("/usr/bin/bash"));
            assert!(is_valid_shell("/bin/zsh"));
            assert!(!is_valid_shell("powershell"));
            assert!(!is_valid_shell("cmd"));
            assert!(!is_valid_shell(""));
        }

        #[test]
        fn test_is_valid_shell_flag() {
            assert!(is_valid_shell_flag("-c"));
            assert!(is_valid_shell_flag("-lc"));
            assert!(is_valid_shell_flag("-lic"));
            assert!(!is_valid_shell_flag("-x"));
            assert!(!is_valid_shell_flag(""));
            assert!(!is_valid_shell_flag("--login"));
        }

        #[test]
        fn test_default_flag_for_shell() {
            assert_eq!(default_flag_for_shell("sh"), "-c");
            assert_eq!(default_flag_for_shell("dash"), "-c");
            assert_eq!(default_flag_for_shell("/bin/dash"), "-c");
            assert_eq!(default_flag_for_shell("fish"), "-lc");
            assert_eq!(default_flag_for_shell("bash"), "-lic");
            assert_eq!(default_flag_for_shell("zsh"), "-lic");
            assert_eq!(default_flag_for_shell("/usr/bin/zsh"), "-lic");
        }

        #[test]
        fn test_is_valid_wsl_distro_name() {
            assert!(is_valid_wsl_distro_name("Ubuntu"));
            assert!(is_valid_wsl_distro_name("Ubuntu-22.04"));
            assert!(is_valid_wsl_distro_name("my_distro"));
            assert!(!is_valid_wsl_distro_name(""));
            assert!(!is_valid_wsl_distro_name("distro with spaces"));
            assert!(!is_valid_wsl_distro_name(&"a".repeat(65)));
        }
    }

    #[test]
    fn opencode_extra_search_paths_includes_install_and_fallback_dirs() {
        let home = PathBuf::from("/home/tester");
        let install_dir = Some(std::ffi::OsString::from("/custom/opencode/bin"));
        let xdg_bin_dir = Some(std::ffi::OsString::from("/xdg/bin"));
        let gopath =
            std::env::join_paths([PathBuf::from("/go/path1"), PathBuf::from("/go/path2")]).ok();

        let paths = opencode_extra_search_paths(&home, install_dir, xdg_bin_dir, gopath);

        assert_eq!(paths[0], PathBuf::from("/custom/opencode/bin"));
        assert_eq!(paths[1], PathBuf::from("/xdg/bin"));
        assert!(paths.contains(&PathBuf::from("/home/tester/bin")));
        assert!(paths.contains(&PathBuf::from("/home/tester/.opencode/bin")));
        assert!(paths.contains(&PathBuf::from("/home/tester/.bun/bin")));
        assert!(paths.contains(&PathBuf::from("/home/tester/go/bin")));
        assert!(paths.contains(&PathBuf::from("/go/path1/bin")));
        assert!(paths.contains(&PathBuf::from("/go/path2/bin")));
    }

    #[test]
    fn opencode_extra_search_paths_deduplicates_repeated_entries() {
        let home = PathBuf::from("/home/tester");
        let same_dir = Some(std::ffi::OsString::from("/same/path"));

        let paths = opencode_extra_search_paths(&home, same_dir.clone(), same_dir, None);

        let count = paths
            .iter()
            .filter(|path| path.as_path() == Path::new("/same/path"))
            .count();
        assert_eq!(count, 1);
    }

    #[test]
    fn opencode_extra_search_paths_deduplicates_bun_default_dir() {
        let home = PathBuf::from("/home/tester");
        let paths = opencode_extra_search_paths(&home, None, None, None);

        let count = paths
            .iter()
            .filter(|path| path.as_path() == Path::new("/home/tester/.bun/bin"))
            .count();
        assert_eq!(count, 1);
    }

    #[test]
    fn grok_extra_search_paths_prefers_override_then_default_native_dir() {
        let home = PathBuf::from("/home/tester");
        let paths =
            grok_extra_search_paths(&home, Some(std::ffi::OsString::from("/custom/grok/bin")));

        assert_eq!(paths[0], PathBuf::from("/custom/grok/bin"));
        assert_eq!(paths[1], PathBuf::from("/home/tester/.grok/bin"));
    }

    #[test]
    fn grok_extra_search_paths_deduplicates_default_override() {
        let home = PathBuf::from("/home/tester");
        let paths = grok_extra_search_paths(
            &home,
            Some(std::ffi::OsString::from("/home/tester/.grok/bin")),
        );

        assert_eq!(paths, vec![PathBuf::from("/home/tester/.grok/bin")]);
    }

    #[test]
    fn cli_path_env_search_paths_include_path_entries_and_dedupe() {
        let temp = tempfile::tempdir().expect("temp dir should be created");
        let first = temp.path().join("first");
        let second = temp.path().join("second");
        std::fs::create_dir_all(&first).expect("first dir should be created");
        std::fs::create_dir_all(&second).expect("second dir should be created");

        let path_env = std::env::join_paths([first.clone(), second.clone(), first.clone()])
            .expect("test path env should be joinable");
        let mut paths = vec![first.clone()];

        extend_from_cli_path_env(&mut paths, Some(path_env));

        assert!(paths.contains(&second));
        assert_eq!(paths.iter().filter(|path| *path == &first).count(), 1);
    }

    #[test]
    fn child_search_paths_include_existing_children_with_suffix() {
        let temp = tempfile::tempdir().expect("temp dir should be created");
        let base = temp.path().join("node");
        let bin = base.join("25.8.0").join("bin");
        std::fs::create_dir_all(&bin).expect("version bin should be created");

        let mut paths = Vec::new();
        extend_existing_child_search_paths(&mut paths, &base, Some("bin"));

        assert!(paths.contains(&bin));
    }

    #[test]
    fn env_child_dir_appends_child_and_dedupes() {
        let base = std::ffi::OsString::from("/custom/toolchain");
        let mut paths = Vec::new();

        push_env_child_dir(&mut paths, Some(base.clone()), "bin");
        push_env_child_dir(&mut paths, Some(base), "bin");

        assert_eq!(paths, vec![PathBuf::from("/custom/toolchain").join("bin")]);
    }

    #[cfg(target_os = "windows")]
    #[test]
    fn cli_path_env_skips_windows_apps_alias_dir() {
        assert!(is_windows_app_execution_alias_dir(Path::new(
            r"C:\Users\tester\AppData\Local\Microsoft\WindowsApps"
        )));
        assert!(!is_windows_app_execution_alias_dir(Path::new(
            r"C:\Users\tester\AppData\Roaming\npm"
        )));
    }

    #[cfg(target_os = "windows")]
    #[test]
    fn merge_path_segments_win_preserves_order_and_dedupes_case_insensitively() {
        let merged = merge_path_segments_win(&[
            r"C:\a;C:\B;%SystemRoot%\system32",
            r"C:\b;C:\a", // dup of C:\a (case-insensitive) and C:\B
            "",
        ]);
        assert_eq!(merged, r"C:\a;C:\B;%SystemRoot%\system32");
    }

    #[cfg(unix)]
    #[test]
    fn prepend_search_dir_to_path_preserves_non_utf8_bytes() {
        use std::os::unix::ffi::{OsStrExt, OsStringExt};

        let current = std::ffi::OsString::from_vec(b"/usr/bin:/tmp/\xff/bin".to_vec());
        let combined = prepend_search_dir_to_path(Path::new("/candidate/bin"), &current);

        assert_eq!(
            combined.as_os_str().as_bytes(),
            b"/candidate/bin:/usr/bin:/tmp/\xff/bin"
        );
    }

    #[cfg(target_os = "windows")]
    #[test]
    fn expand_env_chars_preserves_unknown_vars_and_plain_text() {
        // No percent signs -> verbatim.
        assert_eq!(
            expand_env_chars(r"C:\Program Files\nodejs"),
            r"C:\Program Files\nodejs"
        );
        // Undefined variable is preserved verbatim (nothing dropped).
        assert_eq!(
            expand_env_chars(r"D:\npm-global\%DEFINITELY_NOT_A_REAL_VAR_xyz%\bin"),
            r"D:\npm-global\%DEFINITELY_NOT_A_REAL_VAR_xyz%\bin"
        );
        // Empty percent pair is not treated as a variable name.
        assert_eq!(expand_env_chars(r"C:\path\%%\tail"), r"C:\path\%%\tail");
    }

    #[cfg(target_os = "windows")]
    #[test]
    fn build_tool_search_paths_includes_standalone_installer_dirs() {
        // Non-npm installer locations must be scanned even when the process PATH
        // dropped them (regression guard for #6061 / #6278 / #6047).
        let local_data = dirs::data_local_dir().expect("LOCALAPPDATA should resolve");

        let codex_paths = build_tool_search_paths("codex");
        assert!(codex_paths.contains(
            &local_data
                .join("Programs")
                .join("OpenAI")
                .join("Codex")
                .join("bin")
        ));

        let claude_paths = build_tool_search_paths("claude");
        assert!(claude_paths.contains(&local_data.join("Programs").join("claude")));

        // The standalone Codex dir is codex-specific; it must not pollute other tools.
        assert!(!build_tool_search_paths("gemini").contains(
            &local_data
                .join("Programs")
                .join("OpenAI")
                .join("Codex")
                .join("bin")
        ));
    }

    #[cfg(target_os = "windows")]
    #[test]
    fn windows_path_lookup_ignores_same_named_file_in_current_directory() {
        let current_dir = tempfile::tempdir().expect("current directory should be created");
        let path_dir = tempfile::tempdir().expect("PATH directory should be created");
        std::fs::write(current_dir.path().join("codex.cmd"), "@echo current\r\n")
            .expect("current-directory shim should be created");
        let expected = path_dir.path().join("codex.cmd");
        std::fs::write(&expected, "@echo path\r\n").expect("PATH shim should be created");

        let effective_path =
            std::env::join_paths([path_dir.path()]).expect("test PATH should join");
        let output = windows_path_lookup_command("codex", &effective_path)
            .current_dir(current_dir.path())
            .output()
            .expect("where.exe should execute");
        let stderr = decode_command_output(&output.stderr);
        let matches = decode_command_output(&output.stdout)
            .lines()
            .map(str::trim)
            .filter(|line| !line.is_empty())
            .map(PathBuf::from)
            .collect::<Vec<_>>();

        assert!(output.status.success(), "where.exe failed: {stderr}");
        assert_eq!(matches.len(), 1);
        assert_eq!(
            std::fs::canonicalize(&matches[0]).expect("where.exe match should canonicalize"),
            std::fs::canonicalize(&expected).expect("expected PATH shim should canonicalize")
        );
    }

    #[test]
    fn mise_node_search_paths_include_shims_and_installed_node_bins() {
        let temp = tempfile::tempdir().expect("temp dir should be created");
        let home = temp.path();
        let node_bin = home
            .join(".local/share/mise/installs/node/25.8.0")
            .join("bin");
        std::fs::create_dir_all(&node_bin).expect("node bin should be created");

        let mut paths = Vec::new();
        extend_mise_node_search_paths(&mut paths, home);

        assert!(paths.contains(&home.join(".local/share/mise/shims")));
        assert!(paths.contains(&node_bin));
    }

    #[cfg(not(target_os = "windows"))]
    #[test]
    fn tool_executable_candidates_non_windows_uses_plain_binary_name() {
        let dir = PathBuf::from("/usr/local/bin");
        let candidates = tool_executable_candidates("opencode", &dir);

        assert_eq!(candidates, vec![PathBuf::from("/usr/local/bin/opencode")]);
    }

    #[cfg(target_os = "windows")]
    #[test]
    fn tool_executable_candidates_windows_includes_cmd_exe_and_plain_name() {
        let dir = PathBuf::from("C:\\tools");
        let candidates = tool_executable_candidates("opencode", &dir);

        assert_eq!(
            candidates,
            vec![
                PathBuf::from("C:\\tools\\opencode.cmd"),
                PathBuf::from("C:\\tools\\opencode.exe"),
                PathBuf::from("C:\\tools\\opencode"),
            ]
        );
    }

    #[cfg(target_os = "windows")]
    #[test]
    fn tool_executable_candidates_windows_skips_shadowed_npm_unix_shim() {
        let dir = tempfile::tempdir().expect("temp dir should be created");
        let extensionless = dir.path().join("codex");
        let cmd = dir.path().join("codex.cmd");
        std::fs::write(&extensionless, "").expect("extensionless shim should be created");
        std::fs::write(&cmd, "").expect("cmd shim should be created");

        let candidates = tool_executable_candidates("codex", dir.path());

        assert_eq!(candidates, vec![cmd.clone(), dir.path().join("codex.exe")]);
        assert!(!candidates.contains(&extensionless));
    }

    #[cfg(target_os = "windows")]
    #[test]
    fn windows_runnable_sibling_prefers_cmd_over_extensionless_tool() {
        let dir = tempfile::tempdir().expect("temp dir should be created");
        let extensionless = dir.path().join("codex");
        let cmd = dir.path().join("codex.cmd");
        std::fs::write(&extensionless, "").expect("extensionless shim should be created");
        std::fs::write(&cmd, "").expect("cmd shim should be created");

        let preferred = windows_runnable_sibling_for_extensionless_tool(&extensionless);

        assert_eq!(preferred.as_deref(), Some(cmd.as_path()));
    }

    #[cfg(target_os = "windows")]
    #[test]
    fn run_windows_tool_version_command_accepts_canonicalized_cmd_path() {
        let dir = tempfile::tempdir().expect("temp dir should be created");
        let cmd = dir.path().join("codex.cmd");
        std::fs::write(&cmd, "@echo off\r\necho codex-cli 0.144.3\r\n")
            .expect("cmd shim should be created");
        let canonical = std::fs::canonicalize(&cmd).expect("cmd shim should canonicalize");
        assert!(
            canonical.to_string_lossy().starts_with(r"\\?\"),
            "Windows canonical paths should use the verbatim prefix: {}",
            canonical.display()
        );

        let current_path = std::env::var("PATH").unwrap_or_default();
        let output = run_windows_tool_version_command(&canonical, &current_path)
            .expect("canonicalized cmd shim should execute");
        let stderr = decode_command_output(&output.stderr);

        assert!(output.status.success(), "cmd shim failed: {stderr}");
        assert_eq!(
            decode_command_output(&output.stdout).trim(),
            "codex-cli 0.144.3"
        );
    }
}
