use std::path::Path;
use std::sync::Arc;

use serde_json::{json, Value};

use super::claude_accounts::{self, account_from_live, keychain_text, label, SavedAccount};
use super::{cards, is_account_bound};
use crate::app_config::AppType;
use crate::compat::ccswitch::provider::tests_create::TestHome;
use crate::compat::ccswitch::provider::ProviderStore;
use crate::database::Database;
use crate::domain::ToolId;
use crate::provider::Provider as UpstreamProvider;
use crate::store::AppState;

fn store() -> ProviderStore {
    ProviderStore {
        state: AppState::new(Arc::new(Database::memory().expect("memory database"))),
    }
}

fn account(email: &str, org: &str, refresh: &str) -> SavedAccount {
    account_from_live(
        json!({ "claudeAiOauth": {
            "accessToken": format!("at-{email}"),
            "refreshToken": refresh,
            "expiresAt": 1,
            "subscriptionType": "max"
        }}),
        json!({ "emailAddress": email, "organizationUuid": org, "organizationName": "Org" }),
    )
    .expect("account")
}

fn write_live(home: &Path, account: &SavedAccount) {
    std::fs::create_dir_all(home.join(".claude")).expect("claude dir");
    std::fs::write(
        home.join(".claude").join(".credentials.json"),
        serde_json::to_vec(&account.credentials).expect("credentials"),
    )
    .expect("write credentials");
    std::fs::write(
        home.join(".claude.json"),
        serde_json::to_vec(&json!({ "numStartups": 7, "oauthAccount": account.profile }))
            .expect("config"),
    )
    .expect("write config");
}

fn live_refresh_token(home: &Path) -> String {
    let credentials: Value = serde_json::from_slice(
        &std::fs::read(home.join(".claude").join(".credentials.json")).expect("read"),
    )
    .expect("json");
    credentials["claudeAiOauth"]["refreshToken"]
        .as_str()
        .expect("refresh token")
        .to_string()
}

fn live_email(home: &Path) -> String {
    let config: Value =
        serde_json::from_slice(&std::fs::read(home.join(".claude.json")).expect("read"))
            .expect("json");
    assert_eq!(
        config["numStartups"], 7,
        "the rest of ~/.claude.json was lost"
    );
    config["oauthAccount"]["emailAddress"]
        .as_str()
        .expect("email")
        .to_string()
}

fn saved(home: &Path) -> Vec<SavedAccount> {
    claude_accounts::load_from(&home.join("ai-manager").join("claude-accounts.json"))
}

fn relay() -> UpstreamProvider {
    UpstreamProvider::with_id(
        "relay-1".to_string(),
        "Relay".to_string(),
        json!({ "env": {
            "ANTHROPIC_BASE_URL": "https://relay.example.test",
            "ANTHROPIC_AUTH_TOKEN": "sk-relay-0123456789"
        }}),
        None,
    )
}

#[test]
#[serial_test::serial]
fn switching_between_claude_accounts_moves_each_one_in_and_loses_none() {
    let temp = tempfile::tempdir().expect("temp home");
    let home = temp.path();
    let _home = TestHome::set(home);
    let store = store();
    store
        .state
        .db
        .save_provider(AppType::Claude.as_str(), &relay())
        .expect("save relay");
    store
        .state
        .db
        .set_current_provider(AppType::Claude.as_str(), "relay-1")
        .expect("current");

    // Claude Code is signed in to A by its own /login; B is signed in here.
    let a = account("a@example.com", "org-a", "rt-a");
    let b = account("b@example.com", "org-b", "rt-b");
    write_live(home, &a);
    let card_b = claude_accounts::keep(&store, b.clone()).expect("keep b");
    assert_eq!(card_b, cards::card_id(&cards::CLAUDE, &b.id));
    // Signing in again to the same account adds nothing.
    assert_eq!(
        claude_accounts::keep(&store, b.clone()).expect("keep b again"),
        card_b
    );

    // Switching to B keeps A, with an endpoint of its own, then signs in B.
    store
        .switch(ToolId::ClaudeCode, &card_b)
        .expect("switch to b");
    assert_eq!(live_email(home), "b@example.com");
    assert_eq!(live_refresh_token(home), "rt-b");
    let card_a = cards::card_id(&cards::CLAUDE, &a.id);
    let listed = store.list(ToolId::ClaudeCode).expect("list");
    let a_listed = listed.iter().find(|p| p.id == card_a).expect("a kept");
    assert!(a_listed.account_bound);
    assert_eq!(a_listed.name, "Claude · a@example.com");
    assert!(listed.iter().find(|p| p.id == card_b).expect("b").active);
    assert!(
        !listed
            .iter()
            .find(|p| p.id == "relay-1")
            .expect("relay")
            .account_bound
    );

    // Claude Code rotates B's refresh token; switching away keeps the new one.
    let mut rotated = b.clone();
    rotated.credentials["claudeAiOauth"]["refreshToken"] = "rt-b-2".into();
    write_live(home, &rotated);
    store
        .switch(ToolId::ClaudeCode, &card_a)
        .expect("switch to a");
    assert_eq!(live_email(home), "a@example.com");
    assert_eq!(live_refresh_token(home), "rt-a");
    let kept_b = saved(home).into_iter().find(|s| s.id == b.id).expect("b");
    assert_eq!(
        kept_b.credentials["claudeAiOauth"]["refreshToken"],
        "rt-b-2"
    );

    // A relay keeps whoever is signed in; switching back to B brings its
    // rotated token.
    store.switch(ToolId::ClaudeCode, "relay-1").expect("relay");
    assert_eq!(live_email(home), "a@example.com");
    store
        .switch(ToolId::ClaudeCode, &card_b)
        .expect("back to b");
    assert_eq!(live_refresh_token(home), "rt-b-2");

    // Removing an account's endpoint forgets the account.
    store.switch(ToolId::ClaudeCode, "relay-1").expect("relay");
    store.remove(ToolId::ClaudeCode, &card_a).expect("remove a");
    assert!(saved(home).iter().all(|s| s.id != a.id));
    assert!(saved(home).iter().any(|s| s.id == b.id));
}

#[test]
#[serial_test::serial]
fn a_switch_between_relays_reads_nothing_when_no_account_was_signed_in_here() {
    let temp = tempfile::tempdir().expect("temp home");
    let home = temp.path();
    let _home = TestHome::set(home);
    let store = store();
    store
        .state
        .db
        .save_provider(AppType::Claude.as_str(), &relay())
        .expect("save relay");
    write_live(home, &account("a@example.com", "org-a", "rt-a"));
    store.switch(ToolId::ClaudeCode, "relay-1").expect("switch");
    assert!(!home
        .join("ai-manager")
        .join("claude-accounts.json")
        .exists());
    assert!(store
        .list(ToolId::ClaudeCode)
        .expect("list")
        .iter()
        .all(|provider| !provider.account_bound));
}

#[test]
fn a_seat_on_a_team_reads_apart_from_a_personal_plan() {
    let mut team = account("a@example.com", "org-t", "rt");
    team.plan = Some("team".into());
    team.organization = Some("Acme".into());
    assert_eq!(label(&team), "a@example.com · Acme");
    assert_eq!(
        label(&account("a@example.com", "org-p", "rt")),
        "a@example.com"
    );
    assert_ne!(team.id, account("A@example.com", "org-p", "rt").id);
    assert_eq!(
        account("A@Example.com", "org-p", "rt").id,
        account("a@example.com", "org-p", "rt").id
    );
}

#[test]
fn a_keychain_password_printed_as_hex_is_read_back() {
    let text = r#"{"claudeAiOauth":{"accessToken":"x"}}"#;
    let hex: String = format!("{text}\n")
        .bytes()
        .map(|byte| format!("{byte:02x}"))
        .collect();
    assert_eq!(keychain_text(text), serde_json::from_str(text).ok());
    assert_eq!(keychain_text(&hex), serde_json::from_str(text).ok());
    assert_eq!(keychain_text("zz"), None);
}

#[test]
fn only_official_records_bound_to_an_account_count_as_accounts() {
    let mut record = relay();
    record.meta = Some(crate::provider::ProviderMeta {
        auth_binding: Some(crate::provider::AuthBinding {
            source: crate::provider::AuthBindingSource::ManagedAccount,
            auth_provider: Some("codex_oauth".into()),
            account_id: Some("acct".into()),
        }),
        ..Default::default()
    });
    assert!(!is_account_bound(&record), "a custom record is never one");
    record.category = Some("official".into());
    assert!(is_account_bound(&record));
}

#[test]
#[serial_test::serial]
fn a_chatgpt_account_is_an_endpoint_that_signs_codex_in_when_switched_to() {
    let temp = tempfile::tempdir().expect("temp home");
    let _home = TestHome::set(temp.path());
    crate::settings::reload_settings().expect("reload settings");
    let store = store();
    let manager = store.state.codex_oauth_manager.clone();
    tauri::async_runtime::block_on(manager.add_test_account_with_access_token(
        "acct-1",
        "access-1",
        Some("id-1"),
    ))
    .expect("seed account");

    let card = super::codex::card(&store, "acct-1@example.test", "acct-1").expect("card");
    assert_eq!(
        super::codex::card(&store, "acct-1@example.test", "acct-1").expect("again"),
        card
    );
    store.switch(ToolId::Codex, &card).expect("switch");
    let auth: Value = serde_json::from_slice(
        &std::fs::read(crate::codex_config::get_codex_auth_path()).expect("auth.json"),
    )
    .expect("json");
    assert_eq!(auth["tokens"]["access_token"], "access-1");
    let listed = store.list(ToolId::Codex).expect("list");
    let entry = listed.iter().find(|p| p.id == card).expect("listed");
    assert!(entry.account_bound && entry.active);
    assert_eq!(entry.name, "ChatGPT · acct-1@example.test");

    // Removing the endpoint forgets the account.
    let official = {
        let _mutation = store.lock_mutation();
        crate::compat::ccswitch::provider::tool_login::restore(&store, ToolId::Codex)
            .expect("official")
            .created_provider_id
    };
    store.switch(ToolId::Codex, &official).expect("switch away");
    store.remove(ToolId::Codex, &card).expect("remove");
    assert!(tauri::async_runtime::block_on(manager.list_accounts()).is_empty());
}
