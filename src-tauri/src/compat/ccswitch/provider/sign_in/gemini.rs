//! Gemini CLI's Google sign-in, run from AI Manager (ADR-0061):
//! authorization code with PKCE under Gemini CLI's own installed-app client,
//! the tokens written where Gemini CLI keeps them. Switching to the Google
//! official endpoint already tells Gemini CLI to use this sign-in
//! (`security.auth.selectedType = oauth-personal`).
//!
//! The client is read from the Gemini CLI the user installed rather than
//! carried here: it is the CLI's to change, and its "secret" is only a
//! marker Google gives installed apps.

use std::path::Path;
use std::sync::OnceLock;

use regex::bytes::Regex;
use serde::Deserialize;
use serde_json::{json, Value};

use crate::compat::ccswitch::install_probe::probe;
use crate::domain::{SignInFailure, ToolId};

use super::super::ProviderStore;
use super::{post_failure, Pkce};

const AUTHORIZE_URL: &str = "https://accounts.google.com/o/oauth2/v2/auth";
const TOKEN_URL: &str = "https://oauth2.googleapis.com/token";
const USERINFO_URL: &str = "https://www.googleapis.com/oauth2/v2/userinfo?alt=json";
const SCOPES: &str = "https://www.googleapis.com/auth/cloud-platform https://www.googleapis.com/auth/userinfo.email https://www.googleapis.com/auth/userinfo.profile";
pub(super) const CALLBACK_PATHS: &[&str] = &["/oauth2callback"];
/// Gemini CLI's bundle is some 75 MB of chunks named by hash; a directory
/// larger than this is not it.
const MAX_SCAN_BYTES: u64 = 256 * 1024 * 1024;
/// How far from its secret the client id is looked for.
const PAIR_WINDOW: usize = 4096;

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct Client {
    pub id: String,
    pub secret: String,
}

fn secret_pattern() -> &'static Regex {
    static PATTERN: OnceLock<Regex> = OnceLock::new();
    PATTERN.get_or_init(|| Regex::new(r"GOCSPX-[A-Za-z0-9_-]{20,}").expect("static regex"))
}

fn id_pattern() -> &'static Regex {
    static PATTERN: OnceLock<Regex> = OnceLock::new();
    PATTERN.get_or_init(|| {
        Regex::new(r"[0-9]{6,}-[a-z0-9]{20,}\.apps\.googleusercontent\.com").expect("static regex")
    })
}

/// The client in one file of the CLI: its secret, and the client id written
/// nearest to it.
pub(super) fn client_in(source: &[u8]) -> Option<Client> {
    let secret = secret_pattern().find(source)?;
    let from = secret.start().saturating_sub(PAIR_WINDOW);
    let to = (secret.end() + PAIR_WINDOW).min(source.len());
    let id = id_pattern()
        .find_iter(&source[from..to])
        .min_by_key(|found| (from + found.start()).abs_diff(secret.start()))?;
    Some(Client {
        id: String::from_utf8(id.as_bytes().to_vec()).ok()?,
        secret: String::from_utf8(secret.as_bytes().to_vec()).ok()?,
    })
}

fn client_in_dir(dir: &Path) -> Option<Client> {
    let mut files: Vec<_> = std::fs::read_dir(dir)
        .ok()?
        .filter_map(Result::ok)
        .map(|entry| entry.path())
        .filter(|path| {
            path.extension()
                .is_some_and(|ext| ext == "js" || ext == "mjs" || ext == "cjs")
        })
        .collect();
    files.sort();
    let mut scanned = 0u64;
    for path in files {
        let size = std::fs::metadata(&path).map(|meta| meta.len()).unwrap_or(0);
        scanned += size;
        if scanned > MAX_SCAN_BYTES {
            return None;
        }
        if let Some(client) = std::fs::read(&path)
            .ok()
            .and_then(|bytes| client_in(&bytes))
        {
            return Some(client);
        }
    }
    None
}

/// Gemini CLI's client, from the installation a launch would use.
pub(super) async fn client() -> Option<Client> {
    let probed = probe(ToolId::GeminiCli).await.ok()?;
    let entry = probed.entry?;
    let dir = entry.real_path.parent()?.to_path_buf();
    tokio::task::spawn_blocking(move || {
        client_in_dir(&dir).or_else(|| client_in_dir(&dir.join("bundle")))
    })
    .await
    .ok()
    .flatten()
}

pub(super) fn redirect_uri(port: u16) -> String {
    format!("http://127.0.0.1:{port}/oauth2callback")
}

pub(super) fn authorize_url(client: &Client, pkce: &Pkce, redirect: &str) -> String {
    let mut url = url::Url::parse(AUTHORIZE_URL).expect("static URL");
    url.query_pairs_mut()
        .append_pair("client_id", &client.id)
        .append_pair("redirect_uri", redirect)
        .append_pair("response_type", "code")
        .append_pair("scope", SCOPES)
        .append_pair("access_type", "offline")
        .append_pair("prompt", "consent")
        .append_pair("state", &pkce.state)
        .append_pair("code_challenge", &pkce.challenge)
        .append_pair("code_challenge_method", "S256");
    url.into()
}

#[derive(Deserialize)]
struct TokenReply {
    access_token: String,
    #[serde(default)]
    refresh_token: String,
    #[serde(default)]
    expires_in: i64,
    #[serde(default)]
    scope: String,
    #[serde(default)]
    id_token: Option<String>,
}

#[derive(Deserialize)]
struct UserInfo {
    #[serde(default)]
    email: String,
}

/// Signs Gemini CLI in and returns the account with the Google official
/// endpoint.
pub(super) async fn finish(
    store: &ProviderStore,
    client: &Client,
    code: &str,
    pkce: &Pkce,
    redirect: &str,
) -> Result<(String, String), SignInFailure> {
    let http = crate::proxy::http_client::get();
    let response = http
        .post(TOKEN_URL)
        .timeout(super::HTTP_TIMEOUT)
        .form(&[
            ("grant_type", "authorization_code"),
            ("code", code),
            ("redirect_uri", redirect),
            ("client_id", &client.id),
            ("client_secret", &client.secret),
            ("code_verifier", &pkce.verifier),
        ])
        .send()
        .await
        .map_err(|_| SignInFailure::Network)?;
    if !response.status().is_success() {
        return Err(post_failure(response.status()));
    }
    let token: TokenReply = response.json().await.map_err(|_| SignInFailure::Refused)?;
    if token.access_token.is_empty() || token.refresh_token.is_empty() {
        return Err(SignInFailure::Refused);
    }
    let who = http
        .get(USERINFO_URL)
        .timeout(super::HTTP_TIMEOUT)
        .bearer_auth(&token.access_token)
        .send()
        .await
        .map_err(|_| SignInFailure::Network)?;
    if !who.status().is_success() {
        return Err(post_failure(who.status()));
    }
    let email = who
        .json::<UserInfo>()
        .await
        .map_err(|_| SignInFailure::Refused)?
        .email;
    if email.is_empty() {
        return Err(SignInFailure::Refused);
    }

    let now_ms = chrono::Utc::now().timestamp_millis();
    let dir = crate::gemini_config::get_gemini_dir();
    write_sign_in(&dir, &token_file(&token, now_ms), &email)
        .map_err(|_| SignInFailure::SaveFailed)?;
    let created = {
        let _mutation = store.lock_mutation();
        super::super::tool_login::restore(store, ToolId::GeminiCli)
    }
    .map_err(|_| SignInFailure::SaveFailed)?;
    Ok((email, created.created_provider_id))
}

/// `oauth_creds.json` as Gemini CLI writes it.
fn token_file(token: &TokenReply, now_ms: i64) -> Value {
    let mut file = json!({
        "access_token": token.access_token,
        "refresh_token": token.refresh_token,
        "scope": token.scope,
        "token_type": "Bearer",
        "expiry_date": now_ms + token.expires_in.max(0) * 1000,
    });
    if let Some(id_token) = token.id_token.as_deref().filter(|id| !id.is_empty()) {
        file["id_token"] = id_token.into();
    }
    file
}

/// Writes the tokens and names the account active, keeping any account that
/// was active before among the old ones, as Gemini CLI does.
pub(super) fn write_sign_in(dir: &Path, tokens: &Value, email: &str) -> Result<(), ()> {
    let bytes = serde_json::to_vec_pretty(tokens).map_err(|_| ())?;
    crate::config::atomic_write_private(&dir.join("oauth_creds.json"), &bytes).map_err(|_| ())?;

    let accounts_path = dir.join("google_accounts.json");
    let mut accounts = std::fs::read(&accounts_path)
        .ok()
        .and_then(|bytes| serde_json::from_slice::<Value>(&bytes).ok())
        .filter(Value::is_object)
        .unwrap_or_else(|| json!({ "active": null, "old": [] }));
    let previous = accounts
        .get("active")
        .and_then(Value::as_str)
        .map(str::to_string);
    let mut old: Vec<Value> = accounts
        .get("old")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    old.retain(|entry| entry.as_str() != Some(email));
    if let Some(previous) = previous.filter(|previous| previous != email) {
        if !old
            .iter()
            .any(|entry| entry.as_str() == Some(previous.as_str()))
        {
            old.push(previous.into());
        }
    }
    accounts["active"] = email.into();
    accounts["old"] = Value::Array(old);
    let bytes = serde_json::to_vec_pretty(&accounts).map_err(|_| ())?;
    crate::config::atomic_write_private(&accounts_path, &bytes).map_err(|_| ())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_client_is_the_id_written_beside_the_secret() {
        // Shaped like Gemini CLI's bundle, with made-up values; the secret is
        // put together here so no string in the tree looks like a real one.
        let secret = format!("GOCSPX-{}", "x".repeat(28));
        let bundle = format!(
            "{pad}var other=\"111111-{a}.apps.googleusercontent.com\";{far}\
             var OAUTH_CLIENT_ID = \"123456789-{b}.apps.googleusercontent.com\";\
             var OAUTH_CLIENT_SECRET = \"{secret}\";",
            pad = " ".repeat(PAIR_WINDOW * 2),
            a = "a".repeat(32),
            far = " ".repeat(PAIR_WINDOW + 10),
            b = "b".repeat(32),
        );
        let client = client_in(bundle.as_bytes()).expect("client");
        assert_eq!(
            client.id,
            format!("123456789-{}.apps.googleusercontent.com", "b".repeat(32))
        );
        assert_eq!(client.secret, secret);
        assert_eq!(client_in(b"no client here"), None);
    }

    #[test]
    fn signing_in_keeps_the_account_that_was_active_among_the_old() {
        let dir = tempfile::tempdir().expect("tempdir");
        std::fs::write(
            dir.path().join("google_accounts.json"),
            r#"{"active":"before@example.com","old":["older@example.com","now@example.com"]}"#,
        )
        .expect("seed");
        let tokens = json!({ "access_token": "a", "refresh_token": "r" });
        write_sign_in(dir.path(), &tokens, "now@example.com").expect("write");

        let accounts: Value = serde_json::from_slice(
            &std::fs::read(dir.path().join("google_accounts.json")).expect("read"),
        )
        .expect("json");
        assert_eq!(accounts["active"], "now@example.com");
        assert_eq!(
            accounts["old"],
            json!(["older@example.com", "before@example.com"])
        );
        let written: Value = serde_json::from_slice(
            &std::fs::read(dir.path().join("oauth_creds.json")).expect("read"),
        )
        .expect("json");
        assert_eq!(written, tokens);
    }

    #[test]
    fn the_token_file_is_what_gemini_cli_writes() {
        let token = TokenReply {
            access_token: "a".into(),
            refresh_token: "r".into(),
            expires_in: 3599,
            scope: "s".into(),
            id_token: Some("i".into()),
        };
        assert_eq!(
            token_file(&token, 1_000),
            json!({
                "access_token": "a",
                "refresh_token": "r",
                "scope": "s",
                "token_type": "Bearer",
                "expiry_date": 3_600_000,
                "id_token": "i"
            })
        );
    }
}
