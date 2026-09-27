//! Claude accounts signed in from AI Manager, and moving one into Claude Code
//! (ADR-0061).
//!
//! Each account is kept whole, the way Claude Code keeps its own: the
//! credential blob it stores in the Keychain item `Claude Code-credentials`
//! (elsewhere `.credentials.json`), and the `oauthAccount` it writes into
//! `~/.claude.json`. Switching to an account's endpoint puts both back.
//! Claude Code rotates the refresh token as it refreshes, so the copy here of
//! the account it is signed in to is brought up to date before every switch.

use std::path::{Path, PathBuf};
use std::time::Duration;

use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

use crate::domain::{AppError, ErrorCode};
use crate::platform::command::{AllowedProgram, CommandSpec};
use crate::platform::executor::{CommandExecutor, SystemExecutor};
use crate::provider::Provider as UpstreamProvider;

use super::super::{upstream_detail, ProviderStore};
use super::{block_on, bound_account, cards};

/// Names Claude accounts in `authBinding`; upstream reads no binding by it.
pub(in super::super) const AUTH_PROVIDER: &str = "claude_oauth";
const STORE_FILE: &str = "claude-accounts.json";
const KEYCHAIN_SERVICE: &str = "Claude Code-credentials";
const SECURITY_PATH: &str = "/usr/bin/security";
const KEYCHAIN_TIMEOUT: Duration = Duration::from_secs(10);

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct SavedAccount {
    /// The organization and the email: one email can hold a personal plan
    /// and a seat on a team, two subscriptions side by side.
    pub id: String,
    pub email: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub plan: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub organization: Option<String>,
    /// Claude Code's credential blob, `{ "claudeAiOauth": { … } }`.
    pub credentials: Value,
    /// Claude Code's `oauthAccount`.
    pub profile: Value,
}

#[derive(Default, Serialize, Deserialize)]
struct StoreFile {
    #[serde(default)]
    version: u32,
    #[serde(default)]
    accounts: Vec<SavedAccount>,
}

pub(super) fn account_id(email: &str, organization_uuid: &str) -> String {
    format!("{}:{}", organization_uuid, email.to_lowercase())
}

/// How the account reads on its endpoint: the email, and for a seat on a
/// team or enterprise the organization too, so it reads apart from a
/// personal plan of the same email.
pub(super) fn label(account: &SavedAccount) -> String {
    match (account.plan.as_deref(), account.organization.as_deref()) {
        (Some("team" | "enterprise"), Some(org)) if !org.trim().is_empty() => {
            format!("{} · {}", account.email, org.trim())
        }
        _ => account.email.clone(),
    }
}

fn store_path() -> PathBuf {
    crate::infrastructure::paths::product_data_dir().join(STORE_FILE)
}

pub(super) fn load_from(path: &Path) -> Vec<SavedAccount> {
    std::fs::read(path)
        .ok()
        .and_then(|bytes| serde_json::from_slice::<StoreFile>(&bytes).ok())
        .map(|file| file.accounts)
        .unwrap_or_default()
}

pub(super) fn save_to(path: &Path, accounts: &[SavedAccount]) -> Result<(), AppError> {
    let file = StoreFile {
        version: 1,
        accounts: accounts.to_vec(),
    };
    let bytes = serde_json::to_vec_pretty(&file).map_err(|_| save_failed())?;
    crate::config::atomic_write_private(path, &bytes)
        .map_err(|error| save_failed().with_technical(upstream_detail(&error)))
}

fn save_failed() -> AppError {
    AppError::new(ErrorCode::ConfigWriteFailed, "error.provider.saveFailed")
        .with_remediation("error.remediation.retryOrViewDetails")
}

/// Keeps the newer copy of an account, whose refresh token is the live one.
pub(super) fn upsert(accounts: &mut Vec<SavedAccount>, account: SavedAccount) {
    match accounts.iter_mut().find(|saved| saved.id == account.id) {
        Some(saved) => *saved = account,
        None => accounts.push(account),
    }
}

/// Keeps a freshly signed-in account and returns its endpoint.
pub(super) fn keep(store: &ProviderStore, account: SavedAccount) -> Result<String, AppError> {
    let path = store_path();
    let mut accounts = load_from(&path);
    let name = format!("Claude · {}", label(&account));
    let id = account.id.clone();
    upsert(&mut accounts, account);
    save_to(&path, &accounts)?;
    cards::ensure(store, &cards::CLAUDE, &id, &name)
}

/// Forgets an account once no endpoint is bound to it any more.
pub(in super::super) fn forget_unbound(store: &ProviderStore, account: &str) {
    let still_bound = store
        .raw_inventory(crate::domain::ToolId::ClaudeCode)
        .map(|(rows, _)| {
            rows.values()
                .any(|row| bound_account(row, AUTH_PROVIDER).as_deref() == Some(account))
        })
        .unwrap_or(true);
    if still_bound {
        return;
    }
    let path = store_path();
    let mut accounts = load_from(&path);
    let before = accounts.len();
    accounts.retain(|saved| saved.id != account);
    if accounts.len() != before {
        if let Err(error) = save_to(&path, &accounts) {
            log::warn!("claude accounts: could not forget an account: {error:?}");
        }
    }
}

// ---- what Claude Code is signed in to ------------------------------------

/// Claude Code keeps its sign-in in the Keychain on a Mac, unless it was
/// pointed at another config directory, where it uses the file there.
fn uses_keychain() -> bool {
    cfg!(target_os = "macos") && !cfg!(test) && crate::settings::get_claude_override_dir().is_none()
}

fn credentials_path() -> PathBuf {
    crate::config::get_claude_config_dir().join(".credentials.json")
}

fn security(args: Vec<String>) -> CommandSpec {
    CommandSpec::new(AllowedProgram::Security, args)
        .with_program_path(PathBuf::from(SECURITY_PATH))
        .with_timeout(KEYCHAIN_TIMEOUT)
}

/// The account name Claude Code files its Keychain item under.
fn keychain_account() -> String {
    std::env::var("USER")
        .ok()
        .filter(|name| {
            !name.is_empty()
                && name
                    .chars()
                    .all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '_' | '-'))
        })
        .unwrap_or_else(|| "claude-code-user".to_string())
}

/// `security -w` prints a password holding a character it cannot print as
/// hex; Claude Code writes it on one line, but an older write may not be.
pub(super) fn keychain_text(out: &str) -> Option<Value> {
    let out = out.trim();
    if out.starts_with('{') {
        return serde_json::from_str(out).ok();
    }
    if out.len() % 2 != 0 {
        return None;
    }
    let bytes = (0..out.len())
        .step_by(2)
        .map(|i| u8::from_str_radix(&out[i..i + 2], 16).ok())
        .collect::<Option<Vec<u8>>>()?;
    serde_json::from_slice(&bytes).ok()
}

fn read_live_credentials() -> Option<Value> {
    if uses_keychain() {
        let output = block_on(SystemExecutor.execute(security(vec![
            "find-generic-password".into(),
            "-s".into(),
            KEYCHAIN_SERVICE.into(),
            "-w".into(),
        ])))
        .ok()
        .filter(|output| output.success)?;
        return keychain_text(&output.stdout).filter(has_access_token);
    }
    std::fs::read(credentials_path())
        .ok()
        .and_then(|bytes| serde_json::from_slice(&bytes).ok())
        .filter(has_access_token)
}

fn has_access_token(credentials: &Value) -> bool {
    credentials
        .pointer("/claudeAiOauth/accessToken")
        .and_then(Value::as_str)
        .is_some_and(|token| !token.is_empty())
}

fn write_live_credentials(credentials: &Value) -> Result<(), AppError> {
    if uses_keychain() {
        // On one line, as Claude Code writes it: `security -w` returns a
        // password with a newline as hex, which Claude Code reads as none.
        let one_line = serde_json::to_string(credentials).map_err(|_| save_failed())?;
        let spec = security(vec![
            "add-generic-password".into(),
            "-U".into(),
            "-s".into(),
            KEYCHAIN_SERVICE.into(),
            "-a".into(),
            keychain_account(),
            "-w".into(),
            one_line,
        ])
        .with_sensitive_args(vec![7]);
        let output = block_on(SystemExecutor.execute(spec))?;
        if !output.success {
            return Err(save_failed().with_technical(output.technical_detail()));
        }
        return Ok(());
    }
    let bytes = serde_json::to_vec_pretty(credentials).map_err(|_| save_failed())?;
    crate::config::atomic_write_private(&credentials_path(), &bytes)
        .map_err(|error| save_failed().with_technical(upstream_detail(&error)))
}

fn claude_config_path() -> PathBuf {
    crate::config::get_claude_mcp_path()
}

fn read_live_profile() -> Option<Value> {
    let config: Value = serde_json::from_slice(&std::fs::read(claude_config_path()).ok()?).ok()?;
    config
        .get("oauthAccount")
        .filter(|account| account.is_object())
        .cloned()
}

fn write_live_profile(profile: &Value) -> Result<(), AppError> {
    let path = claude_config_path();
    let mut config = match std::fs::read(&path) {
        Ok(bytes) => serde_json::from_slice::<Value>(&bytes).map_err(|_| save_failed())?,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => json!({}),
        Err(error) => return Err(save_failed().with_technical(error.to_string())),
    };
    let object = config.as_object_mut().ok_or_else(save_failed)?;
    object.insert("oauthAccount".to_string(), profile.clone());
    let bytes = serde_json::to_vec_pretty(&config).map_err(|_| save_failed())?;
    crate::config::atomic_write_private(&path, &bytes)
        .map_err(|error| save_failed().with_technical(upstream_detail(&error)))
}

/// The account Claude Code is signed in to, as an account to keep.
fn live_account() -> Option<SavedAccount> {
    let credentials = read_live_credentials()?;
    let profile = read_live_profile()?;
    account_from_live(credentials, profile)
}

pub(super) fn account_from_live(credentials: Value, profile: Value) -> Option<SavedAccount> {
    let email = profile
        .get("emailAddress")
        .and_then(Value::as_str)
        .filter(|email| !email.is_empty())?
        .to_string();
    let organization_uuid = profile
        .get("organizationUuid")
        .and_then(Value::as_str)
        .unwrap_or_default();
    Some(SavedAccount {
        id: account_id(&email, organization_uuid),
        plan: credentials
            .pointer("/claudeAiOauth/subscriptionType")
            .and_then(Value::as_str)
            .map(str::to_string),
        organization: profile
            .get("organizationName")
            .and_then(Value::as_str)
            .map(str::to_string),
        email,
        credentials,
        profile,
    })
}

// ---- switching ----------------------------------------------------------------

/// Before a switch: the copy of the account Claude Code is signed in to is
/// brought up to date. When the switch is about to sign Claude Code in to
/// another account, one the product did not know yet is kept too, with an
/// endpoint of its own, so no sign-in is lost.
pub(in super::super) fn before_switch(
    store: &ProviderStore,
    target: &UpstreamProvider,
) -> Result<(), AppError> {
    let path = store_path();
    let mut accounts = load_from(&path);
    let target_account = bound_account(target, AUTH_PROVIDER);
    // Nobody signed in from here: nothing to keep, and no Keychain read.
    if accounts.is_empty() && target_account.is_none() {
        return Ok(());
    }
    let Some(live) = live_account() else {
        return Ok(());
    };
    let known = accounts.iter().any(|saved| saved.id == live.id);
    let signing_in_elsewhere = target_account.is_some_and(|account| account != live.id);
    if !known && !signing_in_elsewhere {
        return Ok(());
    }
    let name = format!("Claude · {}", label(&live));
    let id = live.id.clone();
    upsert(&mut accounts, live);
    save_to(&path, &accounts)?;
    if !known {
        cards::ensure(store, &cards::CLAUDE, &id, &name)?;
    }
    Ok(())
}

/// After a switch to an account's endpoint, Claude Code is signed in to it.
pub(in super::super) fn after_switch(target: &UpstreamProvider) -> Result<(), AppError> {
    let Some(account) = bound_account(target, AUTH_PROVIDER) else {
        return Ok(());
    };
    let saved = load_from(&store_path())
        .into_iter()
        .find(|saved| saved.id == account)
        .ok_or_else(|| {
            AppError::new(
                ErrorCode::ConfigWriteFailed,
                "error.provider.signInAccountMissing",
            )
            .with_remediation("error.remediation.signInAgain")
        })?;
    write_live_credentials(&saved.credentials)?;
    write_live_profile(&saved.profile)
}
