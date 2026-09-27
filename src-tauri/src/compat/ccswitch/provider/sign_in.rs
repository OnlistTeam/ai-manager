//! Signing in to a tool's subscription from AI Manager (ADR-0061).
//!
//! A sign-in is a flow kept here until it ends: the vendor's page is opened
//! in the browser, a callback on this machine (or the tool's own login
//! command, for Grok Build) waits for it, and the account it brings back is
//! kept where the tool, or upstream, keeps accounts. The dialog follows the
//! flow by id; only the page, the code to type there, the account's name and
//! the endpoint it became ever leave this module.

mod callback;
mod cards;
mod claude;
pub(super) mod claude_accounts;
mod codex;
mod gemini;
mod grok;

use std::collections::HashMap;
use std::future::Future;
use std::sync::{Arc, LazyLock, Mutex};
use std::time::Duration;

use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine as _};
use sha2::{Digest, Sha256};
use tokio::net::TcpListener;
use tokio::sync::watch;

use crate::domain::{
    AppError, ErrorCode, ProviderCreateResult, SignInFailure, SignInPhase, SignInProgress, ToolId,
};
use crate::platform::executor::CommandCancellation;
use crate::provider::Provider as UpstreamProvider;

use self::callback::Arrival;
use super::ProviderStore;

/// Opens the vendor's page in the browser; the command layer owns the opener.
pub type Opener = Arc<dyn Fn(&str) + Send + Sync>;

/// A vendor's token endpoint answers in seconds or not at all.
const HTTP_TIMEOUT: Duration = Duration::from_secs(30);
/// How long a sign-in waits for the browser.
const SIGN_IN_TIMEOUT: Duration = Duration::from_secs(600);

/// The account bound to an official record, for the given kind of account.
pub(super) fn bound_account(raw: &UpstreamProvider, auth_provider: &str) -> Option<String> {
    if raw.category.as_deref() != Some("official") {
        return None;
    }
    raw.meta
        .as_ref()?
        .managed_account_id_for(auth_provider)
        .filter(|account| !account.trim().is_empty())
}

/// Whether the record is one account signed in from AI Manager.
pub(super) fn is_account_bound(raw: &UpstreamProvider) -> bool {
    bound_account(raw, claude_accounts::AUTH_PROVIDER).is_some()
        || bound_account(raw, "codex_oauth").is_some()
}

/// Forgets an account once its last endpoint is gone. The caller holds the
/// provider mutation lock.
pub(super) fn forget_unbound(store: &ProviderStore, tool: ToolId, account: &str) {
    match tool {
        ToolId::ClaudeCode => claude_accounts::forget_unbound(store, account),
        ToolId::Codex => {
            let still_bound = store
                .raw_inventory(tool)
                .map(|(rows, _)| {
                    rows.values()
                        .any(|row| bound_account(row, "codex_oauth").as_deref() == Some(account))
                })
                .unwrap_or(true);
            if !still_bound {
                let manager = store.state.codex_oauth_manager.clone();
                if let Err(error) = block_on(manager.remove_account(account)) {
                    log::warn!("sign-in: could not forget a ChatGPT account: {error}");
                }
            }
        }
        _ => {}
    }
}

/// Runs a future to its end from the synchronous switch path, whether or not
/// that path is on the async runtime.
pub(super) fn block_on<F: Future>(future: F) -> F::Output {
    match tokio::runtime::Handle::try_current() {
        Ok(handle) => tokio::task::block_in_place(|| handle.block_on(future)),
        Err(_) => tauri::async_runtime::block_on(future),
    }
}

pub(super) fn post_failure(status: reqwest::StatusCode) -> SignInFailure {
    if status.is_server_error() {
        SignInFailure::Network
    } else {
        SignInFailure::Refused
    }
}

/// The secrets of one authorization-code round.
pub(super) struct Pkce {
    pub verifier: String,
    pub challenge: String,
    pub state: String,
}

fn random_token(bytes: usize) -> String {
    let mut out = Vec::with_capacity(bytes + 16);
    while out.len() < bytes {
        out.extend_from_slice(uuid::Uuid::new_v4().as_bytes());
    }
    out.truncate(bytes);
    URL_SAFE_NO_PAD.encode(out)
}

impl Pkce {
    fn new() -> Self {
        let verifier = random_token(48);
        let challenge = URL_SAFE_NO_PAD.encode(Sha256::digest(verifier.as_bytes()));
        Self {
            verifier,
            challenge,
            state: random_token(24),
        }
    }
}

// ---- flows ----------------------------------------------------------------------

#[derive(Default)]
struct FlowState {
    over: Option<SignInPhase>,
    url: Option<String>,
    code: Option<String>,
    account: Option<String>,
    failure: Option<SignInFailure>,
    created: Option<String>,
}

pub(super) struct Flow {
    id: String,
    tool: ToolId,
    state: Mutex<FlowState>,
    cancel: watch::Sender<bool>,
    command: Mutex<Option<CommandCancellation>>,
}

impl Flow {
    fn new(tool: ToolId) -> Self {
        Self {
            id: random_token(12),
            tool,
            state: Mutex::new(FlowState::default()),
            cancel: watch::channel(false).0,
            command: Mutex::new(None),
        }
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, FlowState> {
        self.state
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
    }

    fn is_over(&self) -> bool {
        self.lock().over.is_some()
    }

    /// Records the page once; says whether it is new, so it opens once.
    fn set_url(&self, url: String) -> bool {
        let mut state = self.lock();
        if state.url.is_some() || state.over.is_some() {
            return false;
        }
        state.url = Some(url);
        true
    }

    fn set_code(&self, code: String) {
        let mut state = self.lock();
        if state.code.is_none() {
            state.code = Some(code);
        }
    }

    fn hold_command(&self, cancellation: CommandCancellation) {
        *self
            .command
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner) = Some(cancellation);
    }

    /// Records the outcome once: a flow canceled meanwhile stays canceled.
    fn finish(&self, outcome: Result<(String, String), SignInFailure>) {
        let mut state = self.lock();
        if state.over.is_some() {
            return;
        }
        match outcome {
            Ok((account, created)) => {
                state.over = Some(SignInPhase::Done);
                state.account = Some(account).filter(|account| !account.is_empty());
                state.created = Some(created);
            }
            Err(failure) => {
                state.over = Some(SignInPhase::Failed);
                state.failure = Some(failure);
            }
        }
    }

    fn cancel(&self) {
        {
            let mut state = self.lock();
            if state.over.is_none() {
                state.over = Some(SignInPhase::Canceled);
            }
        }
        let _ = self.cancel.send(true);
        if let Some(command) = self
            .command
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .as_ref()
        {
            command.request();
        }
    }

    fn progress(&self, created: Option<ProviderCreateResult>) -> SignInProgress {
        let state = self.lock();
        SignInProgress {
            id: self.id.clone(),
            phase: state.over.unwrap_or(SignInPhase::Waiting),
            url: state.url.clone(),
            code: state.code.clone(),
            account: state.account.clone(),
            failure: state.failure,
            created,
        }
    }
}

static FLOWS: LazyLock<Mutex<HashMap<String, Arc<Flow>>>> =
    LazyLock::new(|| Mutex::new(HashMap::new()));

fn flows() -> std::sync::MutexGuard<'static, HashMap<String, Arc<Flow>>> {
    FLOWS
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
}

fn find(tool: ToolId, id: &str) -> Result<Arc<Flow>, AppError> {
    flows()
        .get(id)
        .filter(|flow| flow.tool == tool)
        .cloned()
        .ok_or_else(|| {
            AppError::new(ErrorCode::ProviderNotFound, "error.provider.signInGone")
                .with_remediation("error.remediation.signInAgain")
        })
}

/// A newer sign-in replaces an older one of the same tool; one that ended is
/// dropped when the next begins.
fn register(flow: Arc<Flow>) {
    let mut flows = flows();
    flows.retain(|_, other| {
        if other.tool == flow.tool {
            other.cancel();
            return false;
        }
        !other.is_over()
    });
    flows.insert(flow.id.clone(), flow);
}

fn unsupported(tool: ToolId) -> AppError {
    AppError::new(
        ErrorCode::ConfigWriteFailed,
        "error.provider.unsupportedTool",
    )
    .with_technical(format!(
        "{} has no sign-in AI Manager can run",
        tool.as_str()
    ))
}

fn loopback_failed(error: std::io::Error) -> AppError {
    AppError::new(ErrorCode::Internal, "error.provider.signInFailed")
        .with_technical(error.to_string())
        .with_remediation("error.remediation.retryOrViewDetails")
}

pub(super) async fn start(
    store: &ProviderStore,
    tool: ToolId,
    open: Opener,
) -> Result<SignInProgress, AppError> {
    let flow = Arc::new(Flow::new(tool));
    let task_store = ProviderStore {
        state: store.state.clone(),
    };
    match tool {
        ToolId::ClaudeCode => {
            let listener = TcpListener::bind("127.0.0.1:0")
                .await
                .map_err(loopback_failed)?;
            let port = listener.local_addr().map_err(loopback_failed)?.port();
            let pkce = Pkce::new();
            let redirect = claude::redirect_uri(port);
            let url = claude::authorize_url(&pkce, &redirect);
            register(flow.clone());
            flow.set_url(url.clone());
            open(&url);
            let task = flow.clone();
            tauri::async_runtime::spawn(async move {
                wait_then(
                    task,
                    listener,
                    claude::CALLBACK_PATHS,
                    pkce,
                    |code, pkce| {
                        Box::pin(async move {
                            let account = claude::exchange(&code, &pkce, &redirect).await?;
                            let _mutation = task_store.lock_mutation();
                            let card = claude_accounts::keep(&task_store, account.clone())
                                .map_err(|_| SignInFailure::SaveFailed)?;
                            refresh_if_current(&task_store, ToolId::ClaudeCode, &card);
                            Ok((account.email, card))
                        })
                    },
                )
                .await;
            });
        }
        ToolId::Codex => {
            register(flow.clone());
            let listener = match codex::listen().await {
                Ok(listener) => listener,
                Err(failure) => {
                    flow.finish(Err(failure));
                    return Ok(flow.progress(None));
                }
            };
            let pkce = Pkce::new();
            let url = codex::authorize_url(&pkce);
            flow.set_url(url.clone());
            open(&url);
            let task = flow.clone();
            tauri::async_runtime::spawn(async move {
                wait_then(task, listener, codex::CALLBACK_PATHS, pkce, |code, pkce| {
                    Box::pin(async move {
                        let (login, account) = codex::keep(&task_store, &code, &pkce).await?;
                        let _mutation = task_store.lock_mutation();
                        let card = codex::card(&task_store, &login, &account)?;
                        refresh_if_current(&task_store, ToolId::Codex, &card);
                        Ok((login, card))
                    })
                })
                .await;
            });
        }
        ToolId::GeminiCli => {
            register(flow.clone());
            let Some(client) = gemini::client().await else {
                flow.finish(Err(SignInFailure::NotInstalled));
                return Ok(flow.progress(None));
            };
            let listener = TcpListener::bind("127.0.0.1:0")
                .await
                .map_err(loopback_failed)?;
            let port = listener.local_addr().map_err(loopback_failed)?.port();
            let pkce = Pkce::new();
            let redirect = gemini::redirect_uri(port);
            let url = gemini::authorize_url(&client, &pkce, &redirect);
            flow.set_url(url.clone());
            open(&url);
            let task = flow.clone();
            tauri::async_runtime::spawn(async move {
                wait_then(
                    task,
                    listener,
                    gemini::CALLBACK_PATHS,
                    pkce,
                    |code, pkce| {
                        Box::pin(async move {
                            gemini::finish(&task_store, &client, &code, &pkce, &redirect).await
                        })
                    },
                )
                .await;
            });
        }
        ToolId::GrokBuild => {
            register(flow.clone());
            tauri::async_runtime::spawn(grok::run(task_store, flow.clone(), open));
        }
        ToolId::OpenCode
        | ToolId::OpenClaw
        | ToolId::Hermes
        | ToolId::Pi
        | ToolId::KimiCode
        | ToolId::DeepSeekDsh => return Err(unsupported(tool)),
    }
    Ok(flow.progress(None))
}

type Finish =
    std::pin::Pin<Box<dyn Future<Output = Result<(String, String), SignInFailure>> + Send>>;

/// Waits for the browser, spends the code, and leaves the browser a page
/// that says how it went.
async fn wait_then(
    flow: Arc<Flow>,
    listener: TcpListener,
    paths: &'static [&'static str],
    pkce: Pkce,
    finish: impl FnOnce(String, Pkce) -> Finish,
) {
    let state = pkce.state.clone();
    let arrival = callback::wait(
        listener,
        paths,
        &state,
        flow.cancel.subscribe(),
        SIGN_IN_TIMEOUT,
    )
    .await;
    match arrival {
        Arrival::Canceled => flow.cancel(),
        Arrival::Failed(failure) => flow.finish(Err(failure)),
        Arrival::Code(code, stream) => {
            let outcome = finish(code, pkce).await;
            if let Err(failure) = &outcome {
                log::warn!(
                    "sign-in for {} did not finish: {failure:?}",
                    flow.tool.as_str()
                );
            }
            callback::reply(stream, outcome.is_ok()).await;
            flow.finish(outcome);
        }
    }
}

/// Signing in again to the account in use puts its fresh sign-in into the
/// tool at once. The caller holds the provider mutation lock.
fn refresh_if_current(store: &ProviderStore, tool: ToolId, card: &str) {
    let current = store
        .raw_inventory(tool)
        .map(|(_, current)| current)
        .unwrap_or_default();
    if current == card {
        if let Err(error) = super::switching::switch(store, tool, card) {
            log::warn!(
                "sign-in: could not refresh the account in use for {}: {error:?}",
                tool.as_str()
            );
        }
    }
}

pub(super) fn status(
    store: &ProviderStore,
    tool: ToolId,
    id: &str,
) -> Result<SignInProgress, AppError> {
    let flow = find(tool, id)?;
    let created = flow.lock().created.clone();
    let created = match created {
        Some(created_provider_id) => Some(ProviderCreateResult {
            providers: store.list(tool)?,
            created_provider_id,
        }),
        None => None,
    };
    Ok(flow.progress(created))
}

pub(super) fn cancel(tool: ToolId, id: &str) {
    if let Ok(flow) = find(tool, id) {
        flow.cancel();
    }
}

/// The page of a sign-in still waiting, to open again.
pub(super) fn page(tool: ToolId, id: &str) -> Result<Option<String>, AppError> {
    let flow = find(tool, id)?;
    let state = flow.lock();
    Ok(state.url.clone().filter(|_| state.over.is_none()))
}

#[cfg(test)]
mod accounts_tests;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pkce_challenge_is_the_s256_of_the_verifier() {
        let pkce = Pkce::new();
        assert!(pkce.verifier.len() >= 43 && pkce.verifier.len() <= 128);
        assert_eq!(
            pkce.challenge,
            URL_SAFE_NO_PAD.encode(Sha256::digest(pkce.verifier.as_bytes()))
        );
        assert_ne!(Pkce::new().state, pkce.state);
    }

    #[test]
    fn a_flow_ends_once_and_a_cancel_is_not_overwritten() {
        let flow = Flow::new(ToolId::Codex);
        assert!(flow.set_url("https://a".into()));
        assert!(!flow.set_url("https://b".into()));
        flow.cancel();
        flow.finish(Ok(("someone@example.com".into(), "card".into())));
        let progress = flow.progress(None);
        assert_eq!(progress.phase, SignInPhase::Canceled);
        assert_eq!(progress.account, None);
        assert_eq!(progress.url.as_deref(), Some("https://a"));
    }

    #[test]
    fn a_newer_sign_in_replaces_the_older_one_of_its_tool() {
        let older = Arc::new(Flow::new(ToolId::GrokBuild));
        let other = Arc::new(Flow::new(ToolId::ClaudeCode));
        register(older.clone());
        register(other.clone());
        let newer = Arc::new(Flow::new(ToolId::GrokBuild));
        register(newer.clone());
        assert_eq!(older.progress(None).phase, SignInPhase::Canceled);
        assert!(find(ToolId::GrokBuild, &older.id).is_err());
        assert!(find(ToolId::GrokBuild, &newer.id).is_ok());
        assert!(find(ToolId::ClaudeCode, &other.id).is_ok());
        // A flow is found only under its own tool.
        assert!(find(ToolId::Codex, &newer.id).is_err());
        other.cancel();
        newer.cancel();
    }
}
