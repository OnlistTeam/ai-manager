//! Runtime for supplying the native layout (ADR-0033): download, verify, unpack, persist,
//! check the signature, switch the launch entry.
//!
//! Every decision (platform package, layout paths, document parsing) comes from
//! `compat::ccswitch::native_supply`; this module only does I/O and subprocesses. A
//! failure at any step deletes only the files written by this run and always leaves the
//! launcher untouched, so an abort halfway through means "nothing happened" to the user.

use std::fs;
use std::io::{self, Read, Write};
use std::path::{Path, PathBuf};
use std::time::Duration;

use futures::StreamExt;
use sha2::{Digest, Sha512};

use crate::adapters::lifecycle_runner::community_mirror_can_help;
use crate::adapters::tool_adapter::LifecycleContext;
use crate::compat::ccswitch::install_probe::InstalledEntry;
use crate::compat::ccswitch::lifecycle::{COMMUNITY_NPM_REGISTRY, OFFICIAL_NPM_REGISTRY};
use crate::compat::ccswitch::lifecycle_specs::tool_program;
use crate::compat::ccswitch::native_supply::{
    code_signing_team, decode_sha512_integrity, layout_for, official_update_probe_url,
    parse_packument_version, registry_url, target_for, NativeLayout, NativeSupplyTarget,
};
use crate::domain::operation::{log_message, phase};
use crate::domain::{AppError, DownloadStrategy, ErrorCode, OperationLogKind, ToolId};
use crate::platform::command::{AllowedProgram, CommandSpec};
use crate::platform::executor::CommandOutput;

const PACKUMENT_TIMEOUT: Duration = Duration::from_secs(30);
const PACKUMENT_MAX_BYTES: u64 = 1024 * 1024;
/// Same order of magnitude as `lifecycle_specs::INSTALL_TIMEOUT`: a full package download takes minutes on a slow network.
const TARBALL_TIMEOUT: Duration = Duration::from_secs(900);
const TARBALL_MAX_BYTES: u64 = 400 * 1024 * 1024;
/// Downloads are batched at this granularity before being handed to the blocking thread
/// pool: one task per network chunk is too fine-grained, while buffering the whole package
/// in memory defeats the purpose of a streaming download.
const WRITE_BATCH_BYTES: usize = 4 * 1024 * 1024;
const VERIFY_TIMEOUT: Duration = Duration::from_secs(60);

/// Upper bound for probing the official channel. It only has to answer "is this route
/// usable"; if it cannot answer, treat the route as unusable — the supply path can install
/// the very same program, so it is better to give up probing early than to make the user
/// wait.
const OFFICIAL_CHANNEL_PROBE_TIMEOUT: Duration = Duration::from_secs(10);
const CODESIGN_PATH: &str = "/usr/bin/codesign";
const STAGING_DIR_NAME: &str = "native-supply";

/// The two registry roots: official first, community mirror as fallback. Production pins
/// them to `OFFICIAL_NPM_REGISTRY` / `COMMUNITY_NPM_REGISTRY`; tests inject local stubs.
pub(crate) struct SupplyEndpoints {
    pub official: String,
    pub community: String,
}

/// Whether the tool's own official update channel is currently reachable.
///
/// On a restricted network `claude update` retries a blocked domain three times and only
/// gives up after a minute and a half, and that wait produces nothing: without the version
/// document it will not download anything. Asking once, for at most 10 seconds, lets the
/// caller go straight to the supply path and turns the same update from "95s failure +
/// download" back into "10s + download".
///
/// The decision is deliberately conservative: only a transport-level failure to connect
/// counts as unreachable, and any HTTP response (including 404/5xx) counts as reachable.
/// Skipping the tool's own update command is a costly decision and is only made on solid
/// evidence; conversely, if the probe succeeds but the self-update still fails, the
/// existing supply-after-failure path still covers it.
pub(crate) async fn official_channel_reachable(id: ToolId) -> bool {
    match official_update_probe_url(id) {
        None => true,
        Some(url) => channel_reachable(url).await,
    }
}

async fn channel_reachable(url: &str) -> bool {
    tokio::time::timeout(OFFICIAL_CHANNEL_PROBE_TIMEOUT, async {
        // HEAD fetches no body; a 405 from a server that does not support it is still a response and still proves the route works.
        client(url).head(url).send().await.is_ok()
    })
    .await
    .unwrap_or(false)
}

/// Production entry point: derive the layout from the detected native installation and supply the target version from the public registry.
pub async fn run(
    id: ToolId,
    target_version: &str,
    entry: &InstalledEntry,
    ctx: &LifecycleContext,
) -> Result<(), AppError> {
    let target = target_for(id, target_version)
        .ok_or_else(|| unsupported("no registry package for this tool on this platform"))?;
    let layout = layout_for(id, entry)
        .ok_or_else(|| unsupported("installation is not the verified official layout"))?;
    let endpoints = SupplyEndpoints {
        official: OFFICIAL_NPM_REGISTRY.to_string(),
        community: COMMUNITY_NPM_REGISTRY.to_string(),
    };
    let staging_dir = crate::infrastructure::paths::product_data_dir().join(STAGING_DIR_NAME);
    supply(id, &target, &layout, &endpoints, &staging_dir, ctx).await
}

pub(crate) async fn supply(
    id: ToolId,
    target: &NativeSupplyTarget,
    layout: &NativeLayout,
    endpoints: &SupplyEndpoints,
    staging_dir: &Path,
    ctx: &LifecycleContext,
) -> Result<(), AppError> {
    ctx.cancellation.checkpoint()?;
    ctx.report(10, phase::DOWNLOADING);
    ctx.log_message(OperationLogKind::System, log_message::NATIVE_SUPPLY_STARTED);
    if ctx.network.proxy_url.is_some() {
        ctx.log_message(
            OperationLogKind::System,
            log_message::USING_CONFIGURED_PROXY,
        );
    }
    {
        let staging_dir = staging_dir.to_path_buf();
        blocking_io(move || {
            fs::create_dir_all(&staging_dir).map_err(|error| {
                layout_failed(format!("could not prepare staging directory: {error}"))
            })
        })
        .await?;
    }

    // The official registry comes first; only the recoverable download failures defined in ADR-0010 switch this operation to the community mirror.
    let tarball = match download(target, &endpoints.official, staging_dir, ctx).await {
        Ok(tarball) => tarball,
        Err(error)
            if ctx.network.download_strategy == DownloadStrategy::Automatic
                && community_mirror_can_help(&error) =>
        {
            ctx.log_message(
                OperationLogKind::System,
                log_message::USING_COMMUNITY_MIRROR,
            );
            download(target, &endpoints.community, staging_dir, ctx).await?
        }
        Err(error) => return Err(error),
    };

    ctx.cancellation.checkpoint()?;
    ctx.report(60, phase::INSTALLING);
    let partial = layout
        .versions_dir
        .join(format!(".{}.partial", target.version));
    let final_file = layout.versions_dir.join(&target.version);
    {
        let partial = partial.clone();
        let binary_name = target.binary_name;
        tokio::task::spawn_blocking(move || extract_binary(&tarball, binary_name, &partial))
            .await
            .map_err(join_failed)??;
    }

    // From here on, any failure deletes the files written by this run. A `.partial` does not occupy the final name until the signature check passes.
    if let Err(error) = verify_signature(id, &partial, ctx).await {
        remove_file_best_effort(&partial);
        return Err(error);
    }

    ctx.report(70, phase::CHECKING);
    let (placed, staged) = {
        let partial = partial.clone();
        let final_file = final_file.clone();
        let layout = layout.clone();
        let binary_name = target.binary_name;
        blocking_io(move || {
            let placed = place_version_file(&partial, &final_file)?;
            match stage_launcher_link(&layout, binary_name, &final_file) {
                Ok(staged) => Ok((placed, staged)),
                Err(error) => Err(placed.undo(error)),
            }
        })
        .await?
    };
    if let Err(error) = verify_version(id, &staged.link, &target.version, ctx).await {
        staged.discard();
        return Err(placed.undo(error));
    }
    if let Err(error) = ctx.cancellation.checkpoint() {
        staged.discard();
        return Err(placed.undo(error));
    }

    ctx.report(80, phase::CONFIGURING);
    let launcher = layout.launcher.clone();
    blocking_io(move || staged.commit(&launcher))
        .await
        .map_err(|error| placed.undo(error))?;
    placed.finish();
    ctx.log_message(OperationLogKind::System, log_message::NATIVE_SUPPLY_DONE);
    Ok(())
}

/// Move a signature-verified `.partial` to its final name. The final name may already
/// exist (a same-version file left behind by a previous supply run or an interrupted
/// official self-update); that file was not written by this run, so it is moved aside and
/// put back unchanged if any later step fails — a failed supply must mean "nothing
/// happened" to the version directory.
fn place_version_file(partial: &Path, final_file: &Path) -> Result<PlacedVersion, AppError> {
    let displaced = if fs::symlink_metadata(final_file).is_ok() {
        let aside = final_file.with_file_name(format!(
            ".{}.displaced-{}",
            final_file
                .file_name()
                .and_then(|name| name.to_str())
                .unwrap_or("version"),
            uuid::Uuid::new_v4().simple()
        ));
        if let Err(error) = fs::rename(final_file, &aside) {
            remove_file_best_effort(partial);
            return Err(layout_failed(format!(
                "could not set aside the existing program: {error}"
            )));
        }
        Some(aside)
    } else {
        None
    };
    if let Err(error) = fs::rename(partial, final_file) {
        remove_file_best_effort(partial);
        if let Some(aside) = &displaced {
            restore_displaced(aside, final_file);
        }
        return Err(layout_failed(format!(
            "could not place the program in the versions directory: {error}"
        )));
    }
    Ok(PlacedVersion {
        final_file: final_file.to_path_buf(),
        displaced,
    })
}

fn restore_displaced(aside: &Path, final_file: &Path) {
    if let Err(error) = fs::rename(aside, final_file) {
        log::warn!(
            "could not restore the displaced native supply file {}: {error}",
            aside.display()
        );
    }
}

/// The final file just placed in the version directory, plus the old file it displaced (if
/// any). Any later failure makes `undo` take back what this run wrote and restore the old
/// file; only a fully successful supply calls `finish`.
struct PlacedVersion {
    final_file: PathBuf,
    displaced: Option<PathBuf>,
}

impl PlacedVersion {
    fn undo(&self, error: AppError) -> AppError {
        remove_file_best_effort(&self.final_file);
        if let Some(aside) = &self.displaced {
            restore_displaced(aside, &self.final_file);
        }
        error
    }

    /// The new file passed the version re-check and took over the launcher: the old file has no reason to exist any more.
    fn finish(self) {
        if let Some(aside) = &self.displaced {
            remove_file_best_effort(aside);
        }
    }
}

/// Filesystem operations always leave the async runtime: writes, renames and symlink
/// creation can all block on a slow disk or a network volume, while the same worker also
/// runs progress events and other IPC commands.
async fn blocking_io<T, F>(work: F) -> Result<T, AppError>
where
    T: Send + 'static,
    F: FnOnce() -> Result<T, AppError> + Send + 'static,
{
    tokio::task::spawn_blocking(work)
        .await
        .map_err(join_failed)?
}

/// Version document -> tarball -> integrity check, all against the same registry. Returns
/// the verified temporary file, which is deleted when it leaves scope.
async fn download(
    target: &NativeSupplyTarget,
    registry: &str,
    staging_dir: &Path,
    ctx: &LifecycleContext,
) -> Result<tempfile::TempPath, AppError> {
    let url = registry_url(registry, target.package, &target.version);
    ctx.log_detail(OperationLogKind::Command, format!("GET {url}"));
    let document = fetch_document(&url).await?;
    let dist = parse_packument_version(&document, &target.version)?;
    let expected = decode_sha512_integrity(&dist.integrity)
        .ok_or_else(|| unavailable("registry integrity is not a sha512 digest"))?;

    ctx.cancellation.checkpoint()?;
    ctx.report(30, phase::DOWNLOADING);
    ctx.log_detail(OperationLogKind::Command, format!("GET {}", dist.tarball));
    let (file, path) = tempfile::Builder::new()
        .prefix("supply-")
        .suffix(".tgz")
        .tempfile_in(staging_dir)
        .map_err(|error| layout_failed(format!("could not create a download file: {error}")))?
        .into_parts();
    let sink = DownloadSink {
        file,
        hasher: Sha512::new(),
    };
    let sink = tokio::time::timeout(TARBALL_TIMEOUT, async move {
        let mut sink = sink;
        let response = client(&dist.tarball)
            .get(&dist.tarball)
            .send()
            .await
            .map_err(|error| network_failed(format!("package download failed: {error}")))?;
        check_status(response.status())?;
        if response
            .content_length()
            .is_some_and(|length| length > TARBALL_MAX_BYTES)
        {
            return Err(unavailable("package exceeds the size limit"));
        }
        let mut stream = response.bytes_stream();
        let mut received: u64 = 0;
        let mut pending: Vec<u8> = Vec::new();
        while let Some(chunk) = stream.next().await {
            ctx.cancellation.checkpoint()?;
            let chunk = chunk.map_err(|error| {
                network_failed(format!("package download interrupted: {error}"))
            })?;
            received = received.saturating_add(chunk.len() as u64);
            if received > TARBALL_MAX_BYTES {
                return Err(unavailable("package exceeds the size limit"));
            }
            pending.extend_from_slice(&chunk);
            if pending.len() >= WRITE_BATCH_BYTES {
                let batch = std::mem::take(&mut pending);
                sink = blocking_io(move || sink.absorb(batch)).await?;
            }
        }
        blocking_io(move || sink.absorb(pending)).await
    })
    .await
    .map_err(|_| network_failed("package download timed out"))??;
    let digest = blocking_io(move || sink.finish()).await?;

    if digest != expected {
        return Err(integrity_failed(
            "downloaded package did not match the registry integrity",
        ));
    }
    Ok(path)
}

/// Downloading to disk and computing the digest both happen in the blocking thread pool: a
/// package can be up to 400 MiB, and neither the write nor the SHA-512 should occupy an
/// async worker.
struct DownloadSink {
    file: fs::File,
    hasher: Sha512,
}

impl DownloadSink {
    fn absorb(mut self, bytes: Vec<u8>) -> Result<Self, AppError> {
        self.hasher.update(&bytes);
        self.file.write_all(&bytes).map_err(|error| {
            layout_failed(format!("could not write the download file: {error}"))
        })?;
        Ok(self)
    }

    fn finish(mut self) -> Result<Vec<u8>, AppError> {
        self.file.flush().map_err(|error| {
            layout_failed(format!("could not finish the download file: {error}"))
        })?;
        Ok(self.hasher.finalize().to_vec())
    }
}

async fn fetch_document(url: &str) -> Result<String, AppError> {
    tokio::time::timeout(PACKUMENT_TIMEOUT, async {
        let response = client(url)
            .get(url)
            .header(reqwest::header::ACCEPT, "application/json")
            .send()
            .await
            .map_err(|error| network_failed(format!("registry request failed: {error}")))?;
        check_status(response.status())?;
        if response
            .content_length()
            .is_some_and(|length| length > PACKUMENT_MAX_BYTES)
        {
            return Err(unavailable("registry response exceeded the size limit"));
        }
        let mut body = Vec::new();
        let mut stream = response.bytes_stream();
        while let Some(chunk) = stream.next().await {
            let chunk = chunk.map_err(|error| {
                network_failed(format!("registry response interrupted: {error}"))
            })?;
            if (body.len() + chunk.len()) as u64 > PACKUMENT_MAX_BYTES {
                return Err(unavailable("registry response exceeded the size limit"));
            }
            body.extend_from_slice(&chunk);
        }
        String::from_utf8(body).map_err(|_| unavailable("registry response was not UTF-8"))
    })
    .await
    .map_err(|_| network_failed("registry request timed out"))?
}

/// Same source as the product's other official endpoint queries: inherits the user's configured local proxy, with loopback targets connecting directly.
fn client(url: &str) -> reqwest::Client {
    crate::compat::ccswitch::network_proxy::http_client_for_fixed_url(url)
}

/// 403/451/5xx are recoverable registry-side failures and may switch mirrors; 404 is the
/// publisher's decision, and other 4xx codes (401/407/429, ...) fail as is per ADR-0010
/// without switching mirrors.
fn check_status(status: reqwest::StatusCode) -> Result<(), AppError> {
    if status.is_success() {
        return Ok(());
    }
    if status == reqwest::StatusCode::NOT_FOUND {
        return Err(unavailable("registry has no such package version"));
    }
    if status == reqwest::StatusCode::FORBIDDEN
        || status == reqwest::StatusCode::UNAVAILABLE_FOR_LEGAL_REASONS
        || status.is_server_error()
    {
        return Err(registry_unavailable(format!("registry returned {status}")));
    }
    Err(unavailable(format!("registry returned {status}")))
}

/// Only the single `package/<binary_name>` entry is extracted, written as a single 0755 file; all other entries are ignored.
fn extract_binary(tarball: &Path, binary_name: &str, destination: &Path) -> Result<(), AppError> {
    let file = fs::File::open(tarball)
        .map_err(|error| layout_failed(format!("could not open the download file: {error}")))?;
    let mut archive = tar::Archive::new(flate2::read::GzDecoder::new(file));
    let wanted = Path::new("package").join(binary_name);
    let entries = archive
        .entries()
        .map_err(|error| layout_failed(format!("package is not a valid archive: {error}")))?;
    for entry in entries {
        let mut entry =
            entry.map_err(|error| layout_failed(format!("package entry unreadable: {error}")))?;
        let path = entry
            .path()
            .map_err(|error| layout_failed(format!("package entry path invalid: {error}")))?
            .into_owned();
        if path.strip_prefix(".").unwrap_or(&path) != wanted {
            continue;
        }
        if !entry.header().entry_type().is_file() {
            return Err(layout_failed("program entry is not a regular file"));
        }
        let mut output = fs::OpenOptions::new()
            .write(true)
            .create(true)
            .truncate(true)
            .open(destination)
            .map_err(|error| layout_failed(format!("could not write the program: {error}")))?;
        let copied = io::copy(&mut (&mut entry).take(TARBALL_MAX_BYTES + 1), &mut output)
            .map_err(|error| layout_failed(format!("could not extract the program: {error}")))?;
        if copied > TARBALL_MAX_BYTES {
            drop(output);
            remove_file_best_effort(destination);
            return Err(layout_failed("program exceeds the size limit"));
        }
        output
            .sync_all()
            .map_err(|error| layout_failed(format!("could not persist the program: {error}")))?;
        drop(output);
        set_executable(destination).map_err(|error| {
            layout_failed(format!("could not mark the program executable: {error}"))
        })?;
        return Ok(());
    }
    Err(layout_failed("package does not contain the program"))
}

/// macOS: the new file must be signed by the vendor's Team ID. Linux relies only on the registry integrity and the version re-check.
async fn verify_signature(id: ToolId, file: &Path, ctx: &LifecycleContext) -> Result<(), AppError> {
    if !cfg!(target_os = "macos") {
        return Ok(());
    }
    let team = code_signing_team(id).ok_or_else(|| unsupported("no vendor signing team"))?;
    let path = file
        .to_str()
        .ok_or_else(|| layout_failed("program path is not valid UTF-8"))?
        .to_string();

    let verify = codesign(vec![
        "--verify".to_string(),
        "--strict".to_string(),
        path.clone(),
    ]);
    let output = execute_logged(ctx, verify).await?;
    if !output.success {
        return Err(signature_failed(format!(
            "codesign --verify --strict rejected the program: {}",
            output.technical_detail()
        )));
    }

    let describe = codesign(vec!["-d".to_string(), "--verbose=2".to_string(), path]);
    let output = execute_logged(ctx, describe).await?;
    let expected = format!("TeamIdentifier={team}");
    let combined = format!("{}\n{}", output.stderr, output.stdout);
    if !output.success || !combined.lines().any(|line| line.trim() == expected) {
        return Err(signature_failed(format!(
            "program is not signed by {team}: {}",
            output.technical_detail()
        )));
    }
    Ok(())
}

fn codesign(args: Vec<String>) -> CommandSpec {
    CommandSpec::new(AllowedProgram::Codesign, args)
        .with_program_path(PathBuf::from(CODESIGN_PATH))
        .with_timeout(VERIFY_TIMEOUT)
}

/// Run `--version` through a temporary symlink named like the launcher: the allowlist
/// requires the file name of the anchored path to be the tool name, while files in the
/// version directory are named after the version.
async fn verify_version(
    id: ToolId,
    link: &Path,
    version: &str,
    ctx: &LifecycleContext,
) -> Result<(), AppError> {
    let spec = CommandSpec::new(tool_program(id), vec!["--version".to_string()])
        .with_program_path(link.to_path_buf())
        .with_timeout(VERIFY_TIMEOUT);
    let output = execute_logged(ctx, spec).await?;
    let reported = output
        .stdout
        .split_whitespace()
        .any(|token| token == version);
    if !output.success || !reported {
        return Err(verify_failed(format!(
            "program did not report {version}: {}",
            output.technical_detail()
        )));
    }
    Ok(())
}

async fn execute_logged(
    ctx: &LifecycleContext,
    spec: CommandSpec,
) -> Result<CommandOutput, AppError> {
    ctx.log_detail(OperationLogKind::Command, spec.redacted_display());
    ctx.executor.execute(spec).await
}

/// A temporary symlink in the launcher's directory. It is used to re-check the version
/// first, then renamed over the launcher: a rename within the same filesystem is atomic, so
/// the launcher never has a moment where it does not exist.
struct StagedLauncherLink {
    dir: PathBuf,
    link: PathBuf,
}

impl StagedLauncherLink {
    fn discard(self) {
        let _ = fs::remove_dir_all(&self.dir);
    }

    fn commit(self, launcher: &Path) -> Result<(), AppError> {
        let result = fs::rename(&self.link, launcher)
            .map_err(|error| switch_failed(format!("could not switch the launcher: {error}")));
        let _ = fs::remove_dir_all(&self.dir);
        result
    }
}

fn stage_launcher_link(
    layout: &NativeLayout,
    binary_name: &str,
    target: &Path,
) -> Result<StagedLauncherLink, AppError> {
    let parent = layout
        .launcher
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
        .ok_or_else(|| switch_failed("launcher has no parent directory"))?;
    fs::symlink_metadata(&layout.launcher)
        .map_err(|error| switch_failed(format!("launcher is unavailable: {error}")))?;
    let dir = parent.join(format!(
        ".ai-manager-supply-{}",
        uuid::Uuid::new_v4().simple()
    ));
    fs::create_dir(&dir)
        .map_err(|error| switch_failed(format!("could not stage the launcher: {error}")))?;
    let link = dir.join(binary_name);
    if let Err(error) = symlink_file(target, &link) {
        let _ = fs::remove_dir_all(&dir);
        return Err(switch_failed(format!(
            "could not create the launcher link: {error}"
        )));
    }
    Ok(StagedLauncherLink { dir, link })
}

#[cfg(unix)]
fn symlink_file(target: &Path, link: &Path) -> io::Result<()> {
    std::os::unix::fs::symlink(target, link)
}

/// Windows never enters the supply path (`target_for` returns `None`); this exists only so both platforms compile.
#[cfg(not(unix))]
fn symlink_file(_target: &Path, _link: &Path) -> io::Result<()> {
    Err(io::Error::new(
        io::ErrorKind::Unsupported,
        "native layout supply is not verified on this platform",
    ))
}

#[cfg(unix)]
fn set_executable(path: &Path) -> io::Result<()> {
    use std::os::unix::fs::PermissionsExt;
    fs::set_permissions(path, fs::Permissions::from_mode(0o755))
}

#[cfg(not(unix))]
fn set_executable(_path: &Path) -> io::Result<()> {
    Ok(())
}

fn remove_file_best_effort(path: &Path) {
    if let Err(error) = fs::remove_file(path) {
        if error.kind() != io::ErrorKind::NotFound {
            log::warn!(
                "could not remove an unverified native supply file {}: {error}",
                path.display()
            );
        }
    }
}

fn join_failed(error: tokio::task::JoinError) -> AppError {
    AppError::new(ErrorCode::Internal, "error.command.joinFailed").with_technical(error.to_string())
}

fn unsupported(technical: impl Into<String>) -> AppError {
    AppError::new(
        ErrorCode::UpdateFailed,
        "error.tool.nativeSupplyUnsupported",
    )
    .with_technical(technical)
    .with_remediation("error.remediation.installManually")
}

fn unavailable(technical: impl Into<String>) -> AppError {
    AppError::new(
        ErrorCode::UpdateFailed,
        "error.tool.nativeSupplyUnavailable",
    )
    .with_technical(technical)
    .with_remediation("error.remediation.retryOrViewDetails")
}

fn network_failed(technical: impl Into<String>) -> AppError {
    AppError::new(ErrorCode::NetworkError, "error.tool.downloadNetworkFailed")
        .with_technical(technical)
        .with_remediation("error.remediation.configureInstallNetwork")
}

fn registry_unavailable(technical: impl Into<String>) -> AppError {
    AppError::new(
        ErrorCode::NetworkError,
        "error.tool.downloadRegistryUnavailable",
    )
    .with_technical(technical)
    .with_remediation("error.remediation.configureInstallNetwork")
}

fn integrity_failed(technical: impl Into<String>) -> AppError {
    AppError::new(ErrorCode::UpdateFailed, "error.tool.nativeSupplyIntegrity")
        .with_technical(technical)
        .with_remediation("error.remediation.retryOrViewDetails")
}

fn layout_failed(technical: impl Into<String>) -> AppError {
    AppError::new(ErrorCode::UpdateFailed, "error.tool.nativeSupplyLayout")
        .with_technical(technical)
        .with_remediation("error.remediation.checkPermissions")
}

fn signature_failed(technical: impl Into<String>) -> AppError {
    AppError::new(ErrorCode::UpdateFailed, "error.tool.nativeSupplySignature")
        .with_technical(technical)
        .with_remediation("error.remediation.retryOrViewDetails")
}

fn verify_failed(technical: impl Into<String>) -> AppError {
    AppError::new(
        ErrorCode::UpdateFailed,
        "error.tool.nativeSupplyVerifyFailed",
    )
    .with_technical(technical)
    .with_remediation("error.remediation.retryOrViewDetails")
}

fn switch_failed(technical: impl Into<String>) -> AppError {
    AppError::new(
        ErrorCode::UpdateFailed,
        "error.tool.nativeSupplySwitchFailed",
    )
    .with_technical(technical)
    .with_remediation("error.remediation.checkPermissions")
}

// The suite fixes a POSIX layout in place: a symlinked launcher and an
// executable bit. Neither has a Windows equivalent, so the module is gated
// rather than rewritten around a platform it does not describe.
#[cfg(all(test, unix))]
#[path = "native_supply/tests.rs"]
mod tests;
