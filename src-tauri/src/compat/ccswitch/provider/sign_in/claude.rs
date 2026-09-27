//! Claude Code's `/login`, run from AI Manager (ADR-0061): authorization
//! code with PKCE under Claude Code's client, the tokens and the profile made
//! into the account Claude Code would have written.

use serde::Deserialize;
use serde_json::{json, Map, Value};

use crate::domain::SignInFailure;

use super::claude_accounts::{account_id, SavedAccount};
use super::{post_failure, Pkce};

const CLIENT_ID: &str = "9d1c250a-e61b-44d9-88ed-5944d1962f5e";
const AUTHORIZE_URL: &str = "https://claude.com/cai/oauth/authorize";
const TOKEN_URL: &str = "https://platform.claude.com/v1/oauth/token";
const PROFILE_URL: &str = "https://api.anthropic.com/api/oauth/profile";
/// What Claude Code asks for at `/login`.
const SCOPES: &str = "org:create_api_key user:profile user:inference user:sessions:claude_code user:mcp_servers user:file_upload user:plugins";

pub(super) const CALLBACK_PATHS: &[&str] = &["/callback"];

pub(super) fn redirect_uri(port: u16) -> String {
    format!("http://localhost:{port}/callback")
}

pub(super) fn authorize_url(pkce: &Pkce, redirect: &str) -> String {
    let mut url = url::Url::parse(AUTHORIZE_URL).expect("static URL");
    url.query_pairs_mut()
        .append_pair("code", "true")
        .append_pair("client_id", CLIENT_ID)
        .append_pair("response_type", "code")
        .append_pair("redirect_uri", redirect)
        .append_pair("scope", SCOPES)
        .append_pair("code_challenge", &pkce.challenge)
        .append_pair("code_challenge_method", "S256")
        .append_pair("state", &pkce.state);
    url.into()
}

#[derive(Deserialize)]
struct TokenReply {
    access_token: String,
    refresh_token: String,
    #[serde(default)]
    expires_in: i64,
    #[serde(default)]
    scope: String,
    #[serde(default)]
    account: TokenAccount,
    #[serde(default)]
    organization: TokenOrganization,
}

#[derive(Default, Deserialize)]
struct TokenAccount {
    #[serde(default)]
    uuid: String,
    #[serde(default)]
    email_address: String,
}

#[derive(Default, Deserialize)]
struct TokenOrganization {
    #[serde(default)]
    uuid: String,
    #[serde(default)]
    name: String,
}

#[derive(Default, Deserialize)]
pub(super) struct Profile {
    #[serde(default)]
    account: ProfileAccount,
    #[serde(default)]
    organization: ProfileOrganization,
}

#[derive(Default, Deserialize)]
struct ProfileAccount {
    #[serde(default)]
    email: String,
    #[serde(default)]
    display_name: String,
}

#[derive(Default, Deserialize)]
struct ProfileOrganization {
    #[serde(default)]
    organization_type: String,
    #[serde(default)]
    rate_limit_tier: String,
    #[serde(default)]
    billing_type: String,
    #[serde(default)]
    has_extra_usage_enabled: bool,
}

/// Claude's organization types, named the way Claude Code names its plans.
fn plan_of(organization_type: &str) -> Option<&'static str> {
    match organization_type {
        "claude_max" => Some("max"),
        "claude_pro" => Some("pro"),
        "claude_team" => Some("team"),
        "claude_enterprise" => Some("enterprise"),
        _ => None,
    }
}

pub(super) async fn exchange(
    code: &str,
    pkce: &Pkce,
    redirect: &str,
) -> Result<SavedAccount, SignInFailure> {
    // A code pasted from Claude's page carries `#state`; one from the
    // callback does not, and either way the state sent is ours.
    let code = code.split('#').next().unwrap_or(code);
    let response = crate::proxy::http_client::get()
        .post(TOKEN_URL)
        .timeout(super::HTTP_TIMEOUT)
        .json(&json!({
            "grant_type": "authorization_code",
            "code": code,
            "redirect_uri": redirect,
            "client_id": CLIENT_ID,
            "code_verifier": pkce.verifier,
            "state": pkce.state,
        }))
        .send()
        .await
        .map_err(|_| SignInFailure::Network)?;
    if !response.status().is_success() {
        return Err(post_failure(response.status()));
    }
    let token: TokenReply = response.json().await.map_err(|_| SignInFailure::Refused)?;
    // The plan comes with the profile, as it does for Claude Code; an
    // account is still kept when the profile does not answer.
    let profile = fetch_profile(&token.access_token).await.unwrap_or_default();
    account_from(token, profile, chrono::Utc::now().timestamp_millis())
}

async fn fetch_profile(access_token: &str) -> Option<Profile> {
    let response = crate::proxy::http_client::get()
        .get(PROFILE_URL)
        .timeout(super::HTTP_TIMEOUT)
        .bearer_auth(access_token)
        .send()
        .await
        .ok()?;
    if !response.status().is_success() {
        return None;
    }
    response.json().await.ok()
}

fn account_from(
    token: TokenReply,
    profile: Profile,
    now_ms: i64,
) -> Result<SavedAccount, SignInFailure> {
    if token.access_token.is_empty() || token.refresh_token.is_empty() {
        return Err(SignInFailure::Refused);
    }
    let email = [profile.account.email, token.account.email_address]
        .into_iter()
        .find(|email| !email.is_empty())
        .ok_or(SignInFailure::Refused)?;
    let plan = plan_of(&profile.organization.organization_type);

    let mut oauth = Map::new();
    oauth.insert("accessToken".into(), token.access_token.into());
    oauth.insert("refreshToken".into(), token.refresh_token.into());
    oauth.insert(
        "expiresAt".into(),
        (now_ms + token.expires_in.max(0) * 1000).into(),
    );
    oauth.insert(
        "scopes".into(),
        token.scope.split_whitespace().collect::<Vec<_>>().into(),
    );
    oauth.insert("subscriptionType".into(), plan.into());
    if !profile.organization.rate_limit_tier.is_empty() {
        oauth.insert(
            "rateLimitTier".into(),
            profile.organization.rate_limit_tier.into(),
        );
    }

    let mut account = Map::new();
    account.insert("accountUuid".into(), token.account.uuid.into());
    account.insert("emailAddress".into(), email.clone().into());
    account.insert(
        "organizationUuid".into(),
        token.organization.uuid.clone().into(),
    );
    if !token.organization.name.is_empty() {
        account.insert(
            "organizationName".into(),
            token.organization.name.clone().into(),
        );
    }
    if !profile.account.display_name.is_empty() {
        account.insert("displayName".into(), profile.account.display_name.into());
    }
    if !profile.organization.billing_type.is_empty() {
        account.insert(
            "billingType".into(),
            profile.organization.billing_type.into(),
        );
    }
    account.insert(
        "hasExtraUsageEnabled".into(),
        profile.organization.has_extra_usage_enabled.into(),
    );

    Ok(SavedAccount {
        id: account_id(&email, &token.organization.uuid),
        email,
        plan: plan.map(str::to_string),
        organization: Some(token.organization.name).filter(|name| !name.is_empty()),
        credentials: json!({ "claudeAiOauth": Value::Object(oauth) }),
        profile: Value::Object(account),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn token() -> TokenReply {
        serde_json::from_value(json!({
            "access_token": "at",
            "refresh_token": "rt",
            "expires_in": 3600,
            "scope": "user:inference user:profile",
            "account": { "uuid": "acc-1", "email_address": "token@example.com" },
            "organization": { "uuid": "org-1", "name": "Team Org" }
        }))
        .expect("token")
    }

    fn profile(kind: &str) -> Profile {
        serde_json::from_value(json!({
            "account": { "email": "someone@example.com", "display_name": "Someone" },
            "organization": {
                "organization_type": kind,
                "rate_limit_tier": "default_claude_max_20x",
                "billing_type": "stripe_subscription",
                "has_extra_usage_enabled": false
            }
        }))
        .expect("profile")
    }

    #[test]
    fn the_account_is_what_claude_code_would_have_written() {
        let account = account_from(token(), profile("claude_max"), 1_000).expect("account");
        assert_eq!(account.id, "org-1:someone@example.com");
        assert_eq!(account.plan.as_deref(), Some("max"));
        assert_eq!(
            account.credentials,
            json!({ "claudeAiOauth": {
                "accessToken": "at",
                "refreshToken": "rt",
                "expiresAt": 3_601_000,
                "scopes": ["user:inference", "user:profile"],
                "subscriptionType": "max",
                "rateLimitTier": "default_claude_max_20x"
            }})
        );
        assert_eq!(account.profile["emailAddress"], "someone@example.com");
        assert_eq!(account.profile["organizationUuid"], "org-1");
        assert_eq!(account.profile["organizationName"], "Team Org");
        assert_eq!(account.profile["displayName"], "Someone");
    }

    #[test]
    fn without_a_profile_the_token_names_the_account() {
        let account = account_from(token(), Profile::default(), 0).expect("account");
        assert_eq!(account.email, "token@example.com");
        assert_eq!(account.plan, None);
        assert_eq!(
            account.credentials["claudeAiOauth"]["subscriptionType"],
            Value::Null
        );
    }

    #[test]
    fn a_reply_without_a_refresh_token_or_an_email_is_refused() {
        let mut no_refresh = token();
        no_refresh.refresh_token.clear();
        assert_eq!(
            account_from(no_refresh, profile("claude_pro"), 0).err(),
            Some(SignInFailure::Refused)
        );
        let mut nobody = token();
        nobody.account.email_address.clear();
        assert_eq!(
            account_from(nobody, Profile::default(), 0).err(),
            Some(SignInFailure::Refused)
        );
    }

    #[test]
    fn the_authorize_url_asks_what_claude_code_asks() {
        let pkce = Pkce {
            verifier: "v".into(),
            challenge: "c".into(),
            state: "s".into(),
        };
        let url = url::Url::parse(&authorize_url(&pkce, &redirect_uri(5555))).expect("url");
        let query: std::collections::HashMap<_, _> = url.query_pairs().into_owned().collect();
        assert_eq!(url.host_str(), Some("claude.com"));
        assert_eq!(query["client_id"], CLIENT_ID);
        assert_eq!(query["redirect_uri"], "http://localhost:5555/callback");
        assert_eq!(query["code_challenge"], "c");
        assert_eq!(query["code_challenge_method"], "S256");
        assert_eq!(query["state"], "s");
        assert!(query["scope"].contains("user:inference"));
    }
}
