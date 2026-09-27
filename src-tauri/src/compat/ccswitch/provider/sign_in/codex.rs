//! Codex's `codex login`, run from AI Manager (ADR-0061): authorization code
//! with PKCE under Codex's client. The tokens go to upstream's
//! `CodexOAuthManager`, which keeps each ChatGPT account and writes it into
//! `auth.json` when its endpoint is switched to.

use std::time::Duration;

use tokio::net::TcpListener;

use crate::domain::SignInFailure;
use crate::proxy::providers::codex_oauth_auth::CodexOAuthError;

use super::super::ProviderStore;
use super::{cards, Pkce};

const CLIENT_ID: &str = "app_EMoamEEZ73f0CkXaXp7hrann";
const AUTHORIZE_URL: &str = "https://auth.openai.com/oauth/authorize";
const SCOPES: &str =
    "openid profile email offline_access api.connectors.read api.connectors.invoke";
/// OpenAI sends Codex's client back to this port and no other.
const CALLBACK_ADDR: &str = "127.0.0.1:1455";
pub(super) const REDIRECT_URI: &str = "http://localhost:1455/auth/callback";
pub(super) const CALLBACK_PATHS: &[&str] = &["/auth/callback"];

/// Takes Codex's callback port. A `codex login` left waiting there is asked
/// to stop first, the way Codex itself frees it.
pub(super) async fn listen() -> Result<TcpListener, SignInFailure> {
    for attempt in 0..10 {
        if let Ok(listener) = TcpListener::bind(CALLBACK_ADDR).await {
            return Ok(listener);
        }
        if attempt == 0 {
            let _ = crate::proxy::http_client::get_for_url("http://127.0.0.1:1455/cancel")
                .get("http://127.0.0.1:1455/cancel")
                .timeout(Duration::from_secs(2))
                .send()
                .await;
        }
        tokio::time::sleep(Duration::from_millis(200)).await;
    }
    Err(SignInFailure::PortBusy)
}

pub(super) fn authorize_url(pkce: &Pkce) -> String {
    let mut url = url::Url::parse(AUTHORIZE_URL).expect("static URL");
    url.query_pairs_mut()
        .append_pair("response_type", "code")
        .append_pair("client_id", CLIENT_ID)
        .append_pair("redirect_uri", REDIRECT_URI)
        .append_pair("scope", SCOPES)
        .append_pair("code_challenge", &pkce.challenge)
        .append_pair("code_challenge_method", "S256")
        .append_pair("id_token_add_organizations", "true")
        .append_pair("codex_cli_simplified_flow", "true")
        .append_pair("state", &pkce.state)
        .append_pair("originator", "codex_cli_rs");
    url.into()
}

/// Keeps the account with upstream; returns its name and upstream's id.
pub(super) async fn keep(
    store: &ProviderStore,
    code: &str,
    pkce: &Pkce,
) -> Result<(String, String), SignInFailure> {
    let account = store
        .state
        .codex_oauth_manager
        .add_account_from_authorization_code(code, &pkce.verifier, REDIRECT_URI)
        .await
        .map_err(|error| match error {
            CodexOAuthError::NetworkError(_) => SignInFailure::Network,
            CodexOAuthError::IoError(_) => SignInFailure::SaveFailed,
            _ => SignInFailure::Refused,
        })?;
    Ok((account.login, account.id))
}

/// The account's endpoint. The caller holds the provider mutation lock.
pub(super) fn card(
    store: &ProviderStore,
    login: &str,
    account: &str,
) -> Result<String, SignInFailure> {
    cards::ensure(store, &cards::CODEX, account, &format!("ChatGPT · {login}"))
        .map_err(|_| SignInFailure::SaveFailed)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_authorize_url_is_the_one_codex_login_opens() {
        let pkce = Pkce {
            verifier: "v".into(),
            challenge: "c".into(),
            state: "s".into(),
        };
        let url = url::Url::parse(&authorize_url(&pkce)).expect("url");
        let query: std::collections::HashMap<_, _> = url.query_pairs().into_owned().collect();
        assert_eq!(url.host_str(), Some("auth.openai.com"));
        assert_eq!(query["client_id"], CLIENT_ID);
        assert_eq!(query["redirect_uri"], "http://localhost:1455/auth/callback");
        assert_eq!(query["code_challenge_method"], "S256");
        assert_eq!(query["originator"], "codex_cli_rs");
        assert_eq!(query["state"], "s");
    }
}
