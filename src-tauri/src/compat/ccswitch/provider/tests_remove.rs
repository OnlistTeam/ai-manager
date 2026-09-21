use std::sync::Arc;

use serde_json::json;

use super::{provider_from_upstream, removal};
use crate::app_config::AppType;
use crate::database::Database;
use crate::domain::{ErrorCode, ToolId};
use crate::provider::{Provider as UpstreamProvider, ProviderMeta};
use crate::store::AppState;

const SECRET: &str = "sk-ant-api03-remove-secret-0123456789ABCD";

fn state() -> AppState {
    AppState::new(Arc::new(
        Database::memory().expect("create removal test database"),
    ))
}

fn claude_provider(id: &str, name: &str) -> UpstreamProvider {
    UpstreamProvider::with_id(
        id.to_string(),
        name.to_string(),
        json!({
            "env": {
                "ANTHROPIC_BASE_URL": "https://relay.example.test",
                "ANTHROPIC_AUTH_TOKEN": SECRET
            }
        }),
        None,
    )
}

fn save(state: &AppState, app_type: AppType, provider: &UpstreamProvider) {
    state
        .db
        .save_provider(app_type.as_str(), provider)
        .expect("save provider");
}

struct TestHome(Option<std::ffi::OsString>);

impl TestHome {
    fn set(path: &std::path::Path) -> Self {
        let previous = std::env::var_os("AI_MANAGER_TEST_HOME");
        std::env::set_var("AI_MANAGER_TEST_HOME", path);
        Self(previous)
    }
}

impl Drop for TestHome {
    fn drop(&mut self) {
        match self.0.take() {
            Some(value) => std::env::set_var("AI_MANAGER_TEST_HOME", value),
            None => std::env::remove_var("AI_MANAGER_TEST_HOME"),
        }
    }
}

#[test]
#[serial_test::serial]
fn an_inactive_service_is_deleted_and_only_the_survivor_comes_back() {
    let state = state();
    let current = claude_provider("remove-current-4f76", "Current");
    let removable = claude_provider("remove-target-9c2d", "Old relay");
    save(&state, AppType::Claude, &current);
    save(&state, AppType::Claude, &removable);
    state
        .db
        .set_current_provider(AppType::Claude.as_str(), &current.id)
        .expect("select current provider");

    let remaining = removal::remove(&state, ToolId::ClaudeCode, &removable.id)
        .expect("remove inactive provider");

    assert!(state
        .db
        .get_provider_by_id(&removable.id, AppType::Claude.as_str())
        .expect("read provider")
        .is_none());
    assert_eq!(remaining.len(), 1);
    assert_eq!(remaining[0].id, current.id);
    assert!(remaining[0].active);
    assert!(!remaining[0].can_remove);
    // The deleted entry disappears from the result together with its key; the surviving one keeps its own.
    let wire = serde_json::to_string(&remaining).expect("serialize product providers");
    assert_eq!(remaining[0].api_key.as_deref(), Some(SECRET));
    assert!(!wire.contains(&removable.id));
    assert!(!wire.contains("settingsConfig"));
}

#[test]
#[serial_test::serial]
fn the_current_service_is_rejected_and_left_unchanged() {
    let state = state();
    let current = claude_provider("remove-active-703a", "Current");
    save(&state, AppType::Claude, &current);
    state
        .db
        .set_current_provider(AppType::Claude.as_str(), &current.id)
        .expect("select current provider");

    let error = removal::remove(&state, ToolId::ClaudeCode, &current.id)
        .expect_err("current provider cannot be removed");

    assert_eq!(error.code, ErrorCode::ConfigWriteFailed);
    assert_eq!(error.message_key, "error.provider.removeActive");
    assert_eq!(
        error.remediation.as_deref(),
        Some("error.remediation.chooseAnotherService")
    );
    assert!(state
        .db
        .get_provider_by_id(&current.id, AppType::Claude.as_str())
        .expect("read provider")
        .is_some());
}

#[test]
fn tool_managed_opencode_services_fail_closed() {
    let state = state();
    let mut managed = UpstreamProvider::with_id(
        "remove-omo-1d8f".to_string(),
        "OMO".to_string(),
        json!({"options": {"apiKey": SECRET}}),
        None,
    );
    managed.category = Some("omo".to_string());
    save(&state, AppType::OpenCode, &managed);

    let product = provider_from_upstream(ToolId::OpenCode, &managed, "");
    assert!(!product.can_remove);

    let error = removal::remove(&state, ToolId::OpenCode, &managed.id)
        .expect_err("tool-managed providers cannot be removed here");
    assert_eq!(error.message_key, "error.provider.removeUnsupported");
    assert_eq!(
        error.remediation.as_deref(),
        Some("error.remediation.removeInTool")
    );
    assert!(state
        .db
        .get_provider_by_id(&managed.id, AppType::OpenCode.as_str())
        .expect("read provider")
        .is_some());
}

#[test]
#[serial_test::serial]
fn opencode_removal_updates_db_and_live_config_but_preserves_unrelated_settings() {
    let temp = tempfile::tempdir().expect("temp home");
    let _home = TestHome::set(temp.path());
    let config_path = crate::opencode_config::get_opencode_config_path();
    std::fs::create_dir_all(config_path.parent().expect("OpenCode config parent"))
        .expect("create OpenCode config directory");
    std::fs::write(
        &config_path,
        serde_json::to_vec_pretty(&json!({
            "theme": "dark",
            "provider": {
                "remove-live-668b": {
                    "npm": "@ai-sdk/openai-compatible",
                    "name": "Old relay",
                    "options": {
                        "baseURL": "https://relay.example.test",
                        "apiKey": SECRET
                    },
                    "models": {}
                },
                "keep-live-a210": {"npm": "@ai-sdk/anthropic"}
            }
        }))
        .expect("serialize OpenCode config"),
    )
    .expect("seed OpenCode config");
    let state = state();
    let mut removable = UpstreamProvider::with_id(
        "remove-live-668b".to_string(),
        "Old relay".to_string(),
        json!({
            "npm": "@ai-sdk/openai-compatible",
            "name": "Old relay",
            "options": {
                "baseURL": "https://relay.example.test",
                "apiKey": SECRET
            },
            "models": {}
        }),
        None,
    );
    removable.meta = Some(ProviderMeta {
        live_config_managed: Some(true),
        ..ProviderMeta::default()
    });
    save(&state, AppType::OpenCode, &removable);

    let remaining =
        removal::remove(&state, ToolId::OpenCode, &removable.id).expect("remove OpenCode provider");

    assert!(remaining.is_empty());
    assert!(state
        .db
        .get_provider_by_id(&removable.id, AppType::OpenCode.as_str())
        .expect("read provider")
        .is_none());
    let live: serde_json::Value =
        serde_json::from_slice(&std::fs::read(&config_path).expect("read OpenCode config"))
            .expect("valid OpenCode config");
    assert_eq!(live["theme"], "dark");
    assert!(live["provider"].get(&removable.id).is_none());
    assert!(live["provider"].get("keep-live-a210").is_some());
}

#[test]
#[serial_test::serial]
fn a_late_opencode_failure_restores_both_db_and_live_config() {
    let temp = tempfile::tempdir().expect("temp home");
    let _home = TestHome::set(temp.path());
    let config_path = crate::opencode_config::get_opencode_config_path();
    std::fs::create_dir_all(config_path.parent().expect("OpenCode config parent"))
        .expect("create OpenCode config directory");
    let settings = json!({
        "npm": "@ai-sdk/openai-compatible",
        "name": "Restore relay",
        "options": {
            "baseURL": "https://relay.example.test",
            "apiKey": SECRET
        },
        "models": {}
    });
    std::fs::write(
        &config_path,
        serde_json::to_vec_pretty(&json!({
            "theme": "dark",
            "provider": {"remove-live-rollback-a42e": settings.clone()}
        }))
        .expect("serialize OpenCode config"),
    )
    .expect("seed OpenCode config");
    let state = state();
    let mut removable = UpstreamProvider::with_id(
        "remove-live-rollback-a42e".to_string(),
        "Restore relay".to_string(),
        settings,
        None,
    );
    removable.meta = Some(ProviderMeta {
        live_config_managed: Some(true),
        ..ProviderMeta::default()
    });
    save(&state, AppType::OpenCode, &removable);
    let before = state
        .db
        .get_provider_by_id(&removable.id, AppType::OpenCode.as_str())
        .expect("read provider")
        .expect("stored provider");

    let error = removal::remove_with(
        &state,
        ToolId::OpenCode,
        &removable.id,
        |state, app_type, id| {
            crate::services::ProviderService::delete(state, app_type, id)?;
            Err(crate::error::AppError::Message(
                "injected post-delete failure".to_string(),
            ))
        },
    )
    .expect_err("injected late failure");

    assert_eq!(error.message_key, "error.provider.removeFailed");
    let after = state
        .db
        .get_provider_by_id(&removable.id, AppType::OpenCode.as_str())
        .expect("read restored provider")
        .expect("provider restored");
    assert_eq!(
        serde_json::to_value(after).expect("serialize restored provider"),
        serde_json::to_value(before).expect("serialize original provider")
    );
    let live: serde_json::Value = serde_json::from_slice(
        &std::fs::read(&config_path).expect("read restored OpenCode config"),
    )
    .expect("valid OpenCode config");
    assert_eq!(live["theme"], "dark");
    assert!(live["provider"].get(&removable.id).is_some());
}

#[test]
fn a_missing_service_returns_the_product_not_found_error() {
    let error = removal::remove(&state(), ToolId::GeminiCli, "remove-missing-0e77")
        .expect_err("missing provider");

    assert_eq!(error.code, ErrorCode::ProviderNotFound);
    assert_eq!(error.message_key, "error.provider.notFound");
    assert_eq!(
        error.technical_message.as_deref(),
        Some("gemini-cli/remove-missing-0e77")
    );
}

#[test]
#[serial_test::serial]
fn a_delete_error_restores_the_exact_provider_snapshot() {
    let state = state();
    let current = claude_provider("remove-rollback-current-f5a1", "Current");
    let removable = claude_provider("remove-rollback-target-99c4", "Restore me");
    save(&state, AppType::Claude, &current);
    save(&state, AppType::Claude, &removable);
    state
        .db
        .set_current_provider(AppType::Claude.as_str(), &current.id)
        .expect("select current provider");
    let before = state
        .db
        .get_provider_by_id(&removable.id, AppType::Claude.as_str())
        .expect("read provider")
        .expect("stored provider");

    let error = removal::remove_with(
        &state,
        ToolId::ClaudeCode,
        &removable.id,
        |state, app_type, id| {
            state.db.delete_provider(app_type.as_str(), id)?;
            Err(crate::error::AppError::Message(format!(
                "delete failed with {SECRET}"
            )))
        },
    )
    .expect_err("injected delete error");

    assert_eq!(error.message_key, "error.provider.removeFailed");
    assert!(!error
        .technical_message
        .as_deref()
        .unwrap_or_default()
        .contains(SECRET));
    let after = state
        .db
        .get_provider_by_id(&removable.id, AppType::Claude.as_str())
        .expect("read restored provider")
        .expect("provider restored");
    assert_eq!(
        serde_json::to_value(after).expect("serialize restored provider"),
        serde_json::to_value(before).expect("serialize original provider")
    );
}

#[test]
#[serial_test::serial]
fn a_delete_panic_is_contained_and_the_provider_is_restored() {
    let state = state();
    let current = claude_provider("remove-panic-current-770e", "Current");
    let removable = claude_provider("remove-panic-target-f86a", "Restore me");
    save(&state, AppType::Claude, &current);
    save(&state, AppType::Claude, &removable);
    state
        .db
        .set_current_provider(AppType::Claude.as_str(), &current.id)
        .expect("select current provider");

    let error = removal::remove_with(
        &state,
        ToolId::ClaudeCode,
        &removable.id,
        |state, app_type, id| -> Result<(), crate::error::AppError> {
            state.db.delete_provider(app_type.as_str(), id)?;
            panic!("delete panicked with {SECRET}");
        },
    )
    .expect_err("injected delete panic");

    assert_eq!(error.code, ErrorCode::Internal);
    assert_eq!(error.message_key, "error.provider.removePanicked");
    assert!(!error
        .technical_message
        .as_deref()
        .unwrap_or_default()
        .contains(SECRET));
    assert!(state
        .db
        .get_provider_by_id(&removable.id, AppType::Claude.as_str())
        .expect("read restored provider")
        .is_some());
}

#[test]
#[serial_test::serial]
fn a_pi_delete_error_that_changed_nothing_is_not_escalated_to_a_restore_failure() {
    let temp = tempfile::tempdir().expect("temp home");
    let _home = TestHome::set(temp.path());
    let state = state();
    let removable = UpstreamProvider::with_id(
        "remove-pi-noop-5e21".to_string(),
        "Pi relay".to_string(),
        json!({
            "name": "Pi relay",
            "baseUrl": "https://relay.example.test/v1",
            "api": "openai-completions",
            "apiKey": SECRET,
            "models": [{"id": "model-a"}]
        }),
        None,
    );
    crate::services::ProviderService::add(&state, AppType::Pi, removable.clone(), true)
        .expect("add Pi service to live");
    let before = state
        .db
        .get_provider_by_id(&removable.id, AppType::Pi.as_str())
        .expect("read provider")
        .expect("stored provider");

    // Upstream `delete` refused before touching anything. Re-adding the row
    // would only fail with "already exists" and turn a plain failure into
    // `removeRestoreFailed`, so nothing must be restored here.
    let error = removal::remove_with(&state, ToolId::Pi, &removable.id, |_, _, _| {
        Err(crate::error::AppError::Message(
            "injected pre-delete failure".to_string(),
        ))
    })
    .expect_err("injected delete error");

    assert_eq!(error.message_key, "error.provider.removeFailed");
    let after = state
        .db
        .get_provider_by_id(&removable.id, AppType::Pi.as_str())
        .expect("read provider")
        .expect("provider untouched");
    assert_eq!(
        serde_json::to_value(after).expect("serialize provider"),
        serde_json::to_value(before).expect("serialize original provider")
    );
    assert!(crate::pi_config::pi_provider_exists(&removable.id).expect("read Pi live"));
}

fn set_auto_failover(state: &AppState, app_type: AppType, enabled: bool) {
    let mut config =
        futures::executor::block_on(state.db.get_proxy_config_for_app(app_type.as_str()))
            .expect("read proxy config");
    config.auto_failover_enabled = enabled;
    futures::executor::block_on(state.db.update_proxy_config_for_app(config))
        .expect("write proxy config");
}

#[test]
#[serial_test::serial]
fn the_final_failover_candidate_is_locked_while_automatic_failover_is_on() {
    let temp = tempfile::tempdir().expect("temp home");
    let _home = TestHome::set(temp.path());
    let state = state();
    let current = claude_provider("remove-failover-current-2b7c", "Current");
    let queued = claude_provider("remove-failover-last-9d1e", "Only backup");
    save(&state, AppType::Claude, &current);
    save(&state, AppType::Claude, &queued);
    state
        .db
        .set_current_provider(AppType::Claude.as_str(), &current.id)
        .expect("select current provider");
    state
        .db
        .add_to_failover_queue(AppType::Claude.as_str(), &queued.id)
        .expect("queue the backup service");
    set_auto_failover(&state, AppType::Claude, true);

    // The routing page refuses to drop the last eligible queue member while
    // automatic failover is on (ADR-0007); deleting the service must not be
    // a side door around the same rule.
    let error = removal::remove(&state, ToolId::ClaudeCode, &queued.id)
        .expect_err("the last failover candidate must stay while failover is on");
    assert_eq!(error.code, ErrorCode::OperationConflict);
    assert_eq!(error.message_key, "error.routing.queueLocked");
    assert!(state
        .db
        .get_provider_by_id(&queued.id, AppType::Claude.as_str())
        .expect("read provider")
        .is_some());

    set_auto_failover(&state, AppType::Claude, false);
    let remaining = removal::remove(&state, ToolId::ClaudeCode, &queued.id)
        .expect("removable once automatic failover is off");
    assert_eq!(remaining.len(), 1);
    assert_eq!(remaining[0].id, current.id);
}
