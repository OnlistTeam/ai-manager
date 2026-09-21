use super::{channel_reachable, official_channel_reachable, supply, SupplyEndpoints};
use crate::adapters::tool_adapter::{LifecycleContext, LifecycleNetworkPolicy};
use crate::compat::ccswitch::native_supply::{NativeLayout, NativeSupplyTarget};
use crate::domain::operation::log_message;
use crate::domain::{AppError, DownloadStrategy, ErrorCode, OperationLogKind, ToolId};
use crate::platform::command::{AllowedProgram, CommandSpec};
use crate::platform::executor::{CommandExecutor, CommandOutput};
use base64::Engine;
use futures::future::BoxFuture;
use sha2::{Digest, Sha512};
use std::io::{ErrorKind, Read, Write};
use std::net::TcpListener;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::thread::JoinHandle;
use std::time::Duration;

const VERSION: &str = "2.1.261";
const PACKAGE: &str = "@anthropic-ai/claude-code-darwin-arm64";
const BINARY: &[u8] = b"#!/bin/sh\necho '2.1.261 (Claude Code)'\n";
const TEAM_LINE: &str =
    "Identifier=com.anthropic.claude-code\nTeamIdentifier=Q6L2SF6YDW\nSealed Resources=none\n";

// ---------------------------------------------------------------- executor

struct ScriptedExecutor {
    outcomes: Mutex<Vec<Result<CommandOutput, AppError>>>,
    seen: Mutex<Vec<CommandSpec>>,
}

impl ScriptedExecutor {
    fn new(outcomes: Vec<Result<CommandOutput, AppError>>) -> Arc<Self> {
        Arc::new(Self {
            outcomes: Mutex::new(outcomes),
            seen: Mutex::new(Vec::new()),
        })
    }

    fn specs(&self) -> Vec<CommandSpec> {
        self.seen.lock().expect("seen lock").clone()
    }
}

impl CommandExecutor for Arc<ScriptedExecutor> {
    fn execute(&self, spec: CommandSpec) -> BoxFuture<'static, Result<CommandOutput, AppError>> {
        self.seen.lock().expect("seen lock").push(spec);
        let mut outcomes = self.outcomes.lock().expect("outcomes lock");
        let next = if outcomes.is_empty() {
            Ok(output(true, "", ""))
        } else {
            outcomes.remove(0)
        };
        Box::pin(async move { next })
    }
}

fn output(success: bool, stdout: &str, stderr: &str) -> CommandOutput {
    CommandOutput {
        success,
        exit_code: Some(if success { 0 } else { 1 }),
        stdout: stdout.to_string(),
        stderr: stderr.to_string(),
    }
}

/// The verification steps a genuine package passes on this platform.
fn happy_scripts() -> Vec<Result<CommandOutput, AppError>> {
    let mut scripts = Vec::new();
    if cfg!(target_os = "macos") {
        scripts.push(Ok(output(true, "", "")));
        scripts.push(Ok(output(true, "", TEAM_LINE)));
    }
    scripts.push(Ok(output(true, "2.1.261 (Claude Code)\n", "")));
    scripts
}

type OperationLog = Arc<Mutex<Vec<(OperationLogKind, Option<String>, Option<String>)>>>;

fn context(
    executor: Arc<ScriptedExecutor>,
    strategy: DownloadStrategy,
) -> (LifecycleContext, OperationLog) {
    let logged: OperationLog = Arc::new(Mutex::new(Vec::new()));
    let log_sink = logged.clone();
    let ctx = LifecycleContext {
        executor: Arc::new(executor),
        cancellation: crate::platform::executor::CommandCancellation::default(),
        progress: Arc::new(|_, _| {}),
        logger: Arc::new(move |kind, message_key, detail| {
            log_sink.lock().expect("operation log lock").push((
                kind,
                message_key.map(str::to_string),
                detail,
            ));
        }),
        network: LifecycleNetworkPolicy::new(strategy, None),
    };
    (ctx, logged)
}

fn logged_keys(log: &OperationLog) -> Vec<String> {
    log.lock()
        .expect("operation log lock")
        .iter()
        .filter_map(|(_, key, _)| key.clone())
        .collect()
}

// ---------------------------------------------------------------- registry

struct MockRegistry {
    address: String,
    requests: Arc<Mutex<Vec<String>>>,
    stop: Arc<AtomicBool>,
    thread: Option<JoinHandle<()>>,
}

impl MockRegistry {
    /// `routes` receives the bound address so a version document can embed the
    /// server's own tarball URL.
    fn serve(routes: impl FnOnce(&str) -> Vec<(String, u16, Vec<u8>)>) -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").expect("bind mock registry");
        listener
            .set_nonblocking(true)
            .expect("nonblocking mock registry");
        let address = format!(
            "http://{}",
            listener.local_addr().expect("registry address")
        );
        let routes = routes(&address);
        let requests: Arc<Mutex<Vec<String>>> = Arc::new(Mutex::new(Vec::new()));
        let stop = Arc::new(AtomicBool::new(false));
        let seen = requests.clone();
        let stop_flag = stop.clone();
        let thread = std::thread::spawn(move || loop {
            if stop_flag.load(Ordering::SeqCst) {
                break;
            }
            match listener.accept() {
                Ok((mut socket, _)) => {
                    socket.set_nonblocking(false).expect("blocking socket");
                    socket
                        .set_read_timeout(Some(Duration::from_secs(2)))
                        .expect("read timeout");
                    let mut head = Vec::new();
                    let mut buffer = [0_u8; 1024];
                    loop {
                        let size = match socket.read(&mut buffer) {
                            Ok(0) | Err(_) => break,
                            Ok(size) => size,
                        };
                        head.extend_from_slice(&buffer[..size]);
                        if head.windows(4).any(|window| window == b"\r\n\r\n") {
                            break;
                        }
                    }
                    let request = String::from_utf8_lossy(&head).into_owned();
                    let path = request
                        .lines()
                        .next()
                        .and_then(|line| line.split_whitespace().nth(1))
                        .unwrap_or_default()
                        .to_string();
                    seen.lock().expect("requests lock").push(path.clone());
                    let (status, body) = routes
                        .iter()
                        .find(|(route, _, _)| *route == path)
                        .map(|(_, status, body)| (*status, body.clone()))
                        .unwrap_or((404, br#"{"error":"Not found"}"#.to_vec()));
                    let reason = match status {
                        200 => "OK",
                        403 => "Forbidden",
                        404 => "Not Found",
                        _ => "Error",
                    };
                    let _ = socket.write_all(
                        format!(
                            "HTTP/1.1 {status} {reason}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                            body.len()
                        )
                        .as_bytes(),
                    );
                    let _ = socket.write_all(&body);
                    let _ = socket.flush();
                }
                Err(error) if error.kind() == ErrorKind::WouldBlock => {
                    std::thread::sleep(Duration::from_millis(5));
                }
                Err(_) => break,
            }
        });
        Self {
            address,
            requests,
            stop,
            thread: Some(thread),
        }
    }

    fn requests(&self) -> Vec<String> {
        self.requests.lock().expect("requests lock").clone()
    }
}

impl Drop for MockRegistry {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::SeqCst);
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}

/// A closed loopback port: every connection is refused immediately.
fn dead_endpoint() -> String {
    let listener = TcpListener::bind("127.0.0.1:0").expect("reserve dead port");
    let address = listener.local_addr().expect("dead address");
    drop(listener);
    format!("http://{address}")
}

fn package_path() -> String {
    format!("/{}/{VERSION}", PACKAGE.replacen('/', "%2f", 1))
}

fn tarball_path() -> String {
    format!("/claude-code-darwin-arm64-{VERSION}.tgz")
}

fn integrity(bytes: &[u8]) -> String {
    format!(
        "sha512-{}",
        base64::engine::general_purpose::STANDARD.encode(Sha512::digest(bytes))
    )
}

fn version_document(tarball_url: &str, integrity: &str) -> Vec<u8> {
    format!(
        r#"{{"name":"{PACKAGE}","version":"{VERSION}","dist":{{"tarball":"{tarball_url}","integrity":"{integrity}"}}}}"#
    )
    .into_bytes()
}

fn package(binary: Option<&[u8]>) -> Vec<u8> {
    let encoder = flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::fast());
    let mut archive = tar::Builder::new(encoder);
    append(
        &mut archive,
        "package/package.json",
        br#"{"name":"x"}"#,
        0o644,
    );
    if let Some(binary) = binary {
        append(&mut archive, "package/claude", binary, 0o755);
    }
    archive
        .into_inner()
        .expect("finish tar")
        .finish()
        .expect("finish gzip")
}

fn append(
    archive: &mut tar::Builder<flate2::write::GzEncoder<Vec<u8>>>,
    path: &str,
    data: &[u8],
    mode: u32,
) {
    let mut header = tar::Header::new_gnu();
    header.set_size(data.len() as u64);
    header.set_mode(mode);
    header.set_entry_type(tar::EntryType::Regular);
    archive
        .append_data(&mut header, path, data)
        .expect("append tar entry");
}

/// A registry that publishes `tarball` under the version document, or a
/// different integrity when the caller wants a mismatch.
fn registry_for(tarball: Vec<u8>, published_integrity: Option<String>) -> MockRegistry {
    let integrity = published_integrity.unwrap_or_else(|| integrity(&tarball));
    MockRegistry::serve(move |address| {
        vec![
            (
                package_path(),
                200,
                version_document(&format!("{address}{}", tarball_path()), &integrity),
            ),
            (tarball_path(), 200, tarball),
        ]
    })
}

// ---------------------------------------------------------------- layout

struct Fixture {
    _home: tempfile::TempDir,
    staging: tempfile::TempDir,
    layout: NativeLayout,
    old_file: PathBuf,
}

fn fixture() -> Fixture {
    let home = tempfile::tempdir().expect("home");
    let versions_dir = home.path().join(".local/share/claude/versions");
    std::fs::create_dir_all(&versions_dir).expect("versions dir");
    let old_file = versions_dir.join("2.1.0");
    std::fs::write(&old_file, b"old").expect("old version");
    let bin = home.path().join(".local/bin");
    std::fs::create_dir_all(&bin).expect("bin dir");
    let launcher = bin.join("claude");
    std::os::unix::fs::symlink(&old_file, &launcher).expect("launcher symlink");
    Fixture {
        _home: home,
        staging: tempfile::tempdir().expect("staging"),
        layout: NativeLayout {
            versions_dir,
            launcher,
        },
        old_file,
    }
}

fn target() -> NativeSupplyTarget {
    NativeSupplyTarget {
        package: PACKAGE,
        version: VERSION.to_string(),
        binary_name: "claude",
    }
}

fn endpoints(official: &str, community: &str) -> SupplyEndpoints {
    SupplyEndpoints {
        official: official.to_string(),
        community: community.to_string(),
    }
}

fn leftovers(dir: &Path, expected: &[&str]) -> Vec<String> {
    let mut names: Vec<String> = std::fs::read_dir(dir)
        .expect("read dir")
        .map(|entry| {
            entry
                .expect("entry")
                .file_name()
                .to_string_lossy()
                .into_owned()
        })
        .filter(|name| !expected.contains(&name.as_str()))
        .collect();
    names.sort();
    names
}

async fn run(
    fixture: &Fixture,
    endpoints: &SupplyEndpoints,
    executor: Arc<ScriptedExecutor>,
    strategy: DownloadStrategy,
) -> (Result<(), AppError>, OperationLog) {
    let (ctx, log) = context(executor, strategy);
    let result = supply(
        ToolId::ClaudeCode,
        &target(),
        &fixture.layout,
        endpoints,
        fixture.staging.path(),
        &ctx,
    )
    .await;
    (result, log)
}

// ---------------------------------------------------------------- tests

#[tokio::test]
async fn a_verified_package_replaces_the_launcher_and_keeps_the_old_version() {
    let fixture = fixture();
    let registry = registry_for(package(Some(BINARY)), None);
    let executor = ScriptedExecutor::new(happy_scripts());

    let (result, log) = run(
        &fixture,
        &endpoints(&registry.address, &dead_endpoint()),
        executor.clone(),
        DownloadStrategy::Automatic,
    )
    .await;
    result.expect("supply succeeds");

    let new_file = fixture.layout.versions_dir.join(VERSION);
    assert_eq!(std::fs::read(&new_file).expect("new version"), BINARY);
    {
        use std::os::unix::fs::PermissionsExt;
        let mode = std::fs::metadata(&new_file)
            .expect("mode")
            .permissions()
            .mode();
        assert_eq!(mode & 0o111, 0o111, "program must be executable");
    }
    assert_eq!(
        std::fs::read_link(&fixture.layout.launcher).expect("launcher link"),
        new_file
    );
    assert_eq!(std::fs::read(&fixture.old_file).expect("old kept"), b"old");
    assert!(leftovers(&fixture.layout.versions_dir, &["2.1.0", VERSION]).is_empty());
    assert!(leftovers(fixture.layout.launcher.parent().unwrap(), &["claude"]).is_empty());
    assert!(leftovers(fixture.staging.path(), &[]).is_empty());
    assert_eq!(registry.requests(), vec![package_path(), tarball_path()]);

    let specs = executor.specs();
    let programs: Vec<AllowedProgram> = specs.iter().map(|spec| spec.program).collect();
    if cfg!(target_os = "macos") {
        assert_eq!(
            programs,
            vec![
                AllowedProgram::Codesign,
                AllowedProgram::Codesign,
                AllowedProgram::ClaudeCode
            ]
        );
        assert_eq!(specs[0].args[..2], ["--verify", "--strict"]);
        assert_eq!(
            specs[0].program_path.as_deref(),
            Some(Path::new("/usr/bin/codesign"))
        );
    } else {
        assert_eq!(programs, vec![AllowedProgram::ClaudeCode]);
    }
    let version_check = specs.last().expect("version check");
    assert_eq!(version_check.args, vec!["--version"]);
    let checked = version_check.program_path.clone().expect("anchored");
    assert_eq!(checked.file_name().unwrap(), "claude");
    version_check.validate().expect("anchored spec is valid");

    let keys = logged_keys(&log);
    assert!(keys
        .iter()
        .any(|key| key == log_message::NATIVE_SUPPLY_STARTED));
    assert!(keys
        .iter()
        .any(|key| key == log_message::NATIVE_SUPPLY_DONE));
    assert!(!keys
        .iter()
        .any(|key| key == log_message::USING_COMMUNITY_MIRROR));
}

#[tokio::test]
async fn an_integrity_mismatch_never_touches_the_layout() {
    let fixture = fixture();
    let registry = registry_for(package(Some(BINARY)), Some(integrity(b"something else")));
    let executor = ScriptedExecutor::new(happy_scripts());

    let (result, _log) = run(
        &fixture,
        &endpoints(&registry.address, &dead_endpoint()),
        executor.clone(),
        DownloadStrategy::Automatic,
    )
    .await;
    let error = result.expect_err("integrity mismatch fails");
    assert_eq!(error.message_key, "error.tool.nativeSupplyIntegrity");
    assert!(leftovers(&fixture.layout.versions_dir, &["2.1.0"]).is_empty());
    assert_eq!(
        std::fs::read_link(&fixture.layout.launcher).unwrap(),
        fixture.old_file
    );
    assert!(executor.specs().is_empty(), "nothing runs before integrity");
    assert!(leftovers(fixture.staging.path(), &[]).is_empty());
}

#[tokio::test]
async fn a_package_without_the_program_is_a_layout_error() {
    let fixture = fixture();
    let registry = registry_for(package(None), None);
    let executor = ScriptedExecutor::new(happy_scripts());

    let (result, _log) = run(
        &fixture,
        &endpoints(&registry.address, &dead_endpoint()),
        executor.clone(),
        DownloadStrategy::Automatic,
    )
    .await;
    let error = result.expect_err("missing program fails");
    assert_eq!(error.message_key, "error.tool.nativeSupplyLayout");
    assert!(leftovers(&fixture.layout.versions_dir, &["2.1.0"]).is_empty());
    assert!(executor.specs().is_empty());
}

#[cfg(target_os = "macos")]
#[tokio::test]
async fn a_rejected_code_signature_removes_the_new_file() {
    let fixture = fixture();
    let registry = registry_for(package(Some(BINARY)), None);
    let executor = ScriptedExecutor::new(vec![Ok(output(false, "", "code signature invalid"))]);

    let (result, _log) = run(
        &fixture,
        &endpoints(&registry.address, &dead_endpoint()),
        executor.clone(),
        DownloadStrategy::Automatic,
    )
    .await;
    let error = result.expect_err("signature failure");
    assert_eq!(error.message_key, "error.tool.nativeSupplySignature");
    assert!(leftovers(&fixture.layout.versions_dir, &["2.1.0"]).is_empty());
    assert_eq!(
        std::fs::read_link(&fixture.layout.launcher).unwrap(),
        fixture.old_file
    );
    assert_eq!(executor.specs().len(), 1);
}

#[cfg(target_os = "macos")]
#[tokio::test]
async fn a_foreign_signing_team_is_rejected_even_when_the_signature_is_valid() {
    let fixture = fixture();
    let registry = registry_for(package(Some(BINARY)), None);
    let executor = ScriptedExecutor::new(vec![
        Ok(output(true, "", "")),
        Ok(output(true, "", "TeamIdentifier=ZZZZZZZZZZ\n")),
    ]);

    let (result, _log) = run(
        &fixture,
        &endpoints(&registry.address, &dead_endpoint()),
        executor.clone(),
        DownloadStrategy::Automatic,
    )
    .await;
    assert_eq!(
        result.expect_err("foreign team").message_key,
        "error.tool.nativeSupplySignature"
    );
    assert!(leftovers(&fixture.layout.versions_dir, &["2.1.0"]).is_empty());
    assert_eq!(executor.specs().len(), 2);
}

#[tokio::test]
async fn a_wrong_reported_version_removes_the_new_file_and_keeps_the_launcher() {
    let fixture = fixture();
    let registry = registry_for(package(Some(BINARY)), None);
    let mut scripts = happy_scripts();
    *scripts.last_mut().unwrap() = Ok(output(true, "2.1.0 (Claude Code)\n", ""));
    let executor = ScriptedExecutor::new(scripts);

    let (result, _log) = run(
        &fixture,
        &endpoints(&registry.address, &dead_endpoint()),
        executor.clone(),
        DownloadStrategy::Automatic,
    )
    .await;
    assert_eq!(
        result.expect_err("version mismatch").message_key,
        "error.tool.nativeSupplyVerifyFailed"
    );
    assert!(leftovers(&fixture.layout.versions_dir, &["2.1.0"]).is_empty());
    assert!(leftovers(fixture.layout.launcher.parent().unwrap(), &["claude"]).is_empty());
    assert_eq!(
        std::fs::read_link(&fixture.layout.launcher).unwrap(),
        fixture.old_file
    );
}

/// A same-version file left behind by an earlier attempt is not this run's
/// to lose: a failed supply must hand it back byte for byte.
#[tokio::test]
async fn a_failed_verification_restores_a_pre_existing_version_file() {
    let fixture = fixture();
    let stale = fixture.layout.versions_dir.join(VERSION);
    std::fs::write(&stale, b"stale").expect("pre-existing version file");
    let registry = registry_for(package(Some(BINARY)), None);
    let mut scripts = happy_scripts();
    *scripts.last_mut().unwrap() = Ok(output(true, "2.1.0 (Claude Code)\n", ""));
    let executor = ScriptedExecutor::new(scripts);

    let (result, _log) = run(
        &fixture,
        &endpoints(&registry.address, &dead_endpoint()),
        executor,
        DownloadStrategy::Automatic,
    )
    .await;
    assert_eq!(
        result.expect_err("version mismatch").message_key,
        "error.tool.nativeSupplyVerifyFailed"
    );
    assert_eq!(std::fs::read(&stale).expect("restored file"), b"stale");
    assert!(leftovers(&fixture.layout.versions_dir, &["2.1.0", VERSION]).is_empty());
    assert_eq!(
        std::fs::read_link(&fixture.layout.launcher).unwrap(),
        fixture.old_file
    );
}

#[tokio::test]
async fn a_pre_existing_version_file_is_replaced_only_by_a_verified_program() {
    let fixture = fixture();
    let stale = fixture.layout.versions_dir.join(VERSION);
    std::fs::write(&stale, b"stale").expect("pre-existing version file");
    let registry = registry_for(package(Some(BINARY)), None);
    let executor = ScriptedExecutor::new(happy_scripts());

    let (result, _log) = run(
        &fixture,
        &endpoints(&registry.address, &dead_endpoint()),
        executor,
        DownloadStrategy::Automatic,
    )
    .await;
    result.expect("supply succeeds");
    assert_eq!(std::fs::read(&stale).expect("verified file"), BINARY);
    assert!(leftovers(&fixture.layout.versions_dir, &["2.1.0", VERSION]).is_empty());
    assert_eq!(std::fs::read_link(&fixture.layout.launcher).unwrap(), stale);
}

#[tokio::test]
async fn official_network_failure_uses_the_mirror_only_in_automatic_mode() {
    let fixture = fixture();
    let community = registry_for(package(Some(BINARY)), None);
    let official = dead_endpoint();

    let executor = ScriptedExecutor::new(happy_scripts());
    let (result, _log) = run(
        &fixture,
        &endpoints(&official, &community.address),
        executor,
        DownloadStrategy::OfficialOnly,
    )
    .await;
    let error = result.expect_err("official-only stays on the official registry");
    assert_eq!(error.code, ErrorCode::NetworkError);
    assert_eq!(error.message_key, "error.tool.downloadNetworkFailed");
    assert!(community.requests().is_empty());
    assert!(leftovers(&fixture.layout.versions_dir, &["2.1.0"]).is_empty());

    let executor = ScriptedExecutor::new(happy_scripts());
    let (result, log) = run(
        &fixture,
        &endpoints(&official, &community.address),
        executor,
        DownloadStrategy::Automatic,
    )
    .await;
    result.expect("automatic mode recovers through the mirror");
    assert_eq!(community.requests(), vec![package_path(), tarball_path()]);
    assert!(logged_keys(&log)
        .iter()
        .any(|key| key == log_message::USING_COMMUNITY_MIRROR));
    assert_eq!(
        std::fs::read_link(&fixture.layout.launcher).unwrap(),
        fixture.layout.versions_dir.join(VERSION)
    );
}

#[tokio::test]
async fn a_public_registry_403_is_retried_through_the_mirror() {
    let fixture = fixture();
    let official = MockRegistry::serve(|_| vec![(package_path(), 403, b"forbidden".to_vec())]);
    let community = registry_for(package(Some(BINARY)), None);
    let executor = ScriptedExecutor::new(happy_scripts());

    let (result, log) = run(
        &fixture,
        &endpoints(&official.address, &community.address),
        executor,
        DownloadStrategy::Automatic,
    )
    .await;
    result.expect("403 on the public registry is recoverable");
    assert_eq!(official.requests(), vec![package_path()]);
    assert_eq!(community.requests(), vec![package_path(), tarball_path()]);
    assert!(logged_keys(&log)
        .iter()
        .any(|key| key == log_message::USING_COMMUNITY_MIRROR));
}

#[tokio::test]
async fn a_missing_platform_version_is_not_retried_on_the_mirror() {
    let fixture = fixture();
    let official = MockRegistry::serve(|_| Vec::new());
    let community = registry_for(package(Some(BINARY)), None);
    let executor = ScriptedExecutor::new(happy_scripts());

    let (result, log) = run(
        &fixture,
        &endpoints(&official.address, &community.address),
        executor,
        DownloadStrategy::Automatic,
    )
    .await;
    let error = result.expect_err("404 is a publisher decision");
    assert_eq!(error.message_key, "error.tool.nativeSupplyUnavailable");
    assert!(community.requests().is_empty());
    assert!(!logged_keys(&log)
        .iter()
        .any(|key| key == log_message::USING_COMMUNITY_MIRROR));
    assert!(leftovers(&fixture.layout.versions_dir, &["2.1.0"]).is_empty());
}

#[tokio::test]
async fn a_pending_cancellation_stops_before_any_download() {
    let fixture = fixture();
    let registry = registry_for(package(Some(BINARY)), None);
    let executor = ScriptedExecutor::new(happy_scripts());
    let (ctx, _log) = context(executor.clone(), DownloadStrategy::Automatic);
    assert!(ctx.cancellation.request());

    let result = supply(
        ToolId::ClaudeCode,
        &target(),
        &fixture.layout,
        &endpoints(&registry.address, &dead_endpoint()),
        fixture.staging.path(),
        &ctx,
    )
    .await;
    assert_eq!(
        result.expect_err("cancelled").message_key,
        "error.operation.cancelled"
    );
    assert!(ctx.cancellation.is_confirmed());
    assert!(registry.requests().is_empty());
    assert!(executor.specs().is_empty());
    assert!(leftovers(&fixture.layout.versions_dir, &["2.1.0"]).is_empty());
}

/// The official-channel probe only answers a transport-level question: any HTTP response counts as
/// reachable (even a 404), and only a failure to connect counts as unreachable. Declaring it
/// "unreachable" makes the product skip the tool's own update command, so the bar has to be high.
#[tokio::test]
async fn a_reachable_official_channel_counts_even_when_it_answers_with_an_error() {
    let registry = MockRegistry::serve(|_| vec![("/latest".to_string(), 404, Vec::new())]);
    assert!(channel_reachable(&format!("{}/latest", registry.address)).await);
}

#[tokio::test]
async fn an_official_channel_that_refuses_the_connection_counts_as_unreachable() {
    assert!(!channel_reachable(&format!("{}/latest", dead_endpoint())).await);
}

/// Tools with no verified probe address are always treated as "reachable": the product never skips their self-update on their behalf.
#[tokio::test]
async fn a_tool_without_a_probe_url_is_never_declared_unreachable() {
    assert!(official_channel_reachable(ToolId::OpenCode).await);
}
