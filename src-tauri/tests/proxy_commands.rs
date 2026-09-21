use ai_manager_lib::AppError;

#[path = "support.rs"]
mod support;
use support::{create_test_state, ensure_test_home, reset_test_fs, test_mutex};

// Tests serialize on a Mutex; holding the lock across await is intentional here.
#[allow(clippy::await_holding_lock)]
#[tokio::test]
async fn default_cost_multiplier_commands_round_trip() {
    let _guard = test_mutex().lock().expect("acquire test mutex");
    reset_test_fs();
    let _home = ensure_test_home();

    let state = create_test_state().expect("create test state");

    let default = state
        .db
        .get_default_cost_multiplier("claude")
        .await
        .expect("read default multiplier");
    assert_eq!(default, "1");

    state
        .db
        .set_default_cost_multiplier("claude", "1.5")
        .await
        .expect("set multiplier");
    let updated = state
        .db
        .get_default_cost_multiplier("claude")
        .await
        .expect("read updated multiplier");
    assert_eq!(updated, "1.5");

    let err = state
        .db
        .set_default_cost_multiplier("claude", "not-a-number")
        .await
        .expect_err("invalid multiplier should error");
    // The error is a Localized variant (i18n-aware).
    match err {
        AppError::Localized { key, .. } => {
            assert_eq!(key, "error.invalidMultiplier");
        }
        other => panic!("expected localized error, got {other:?}"),
    }
}

// Tests serialize on a Mutex; holding the lock across await is intentional here.
#[allow(clippy::await_holding_lock)]
#[tokio::test]
async fn pricing_model_source_commands_round_trip() {
    let _guard = test_mutex().lock().expect("acquire test mutex");
    reset_test_fs();
    let _home = ensure_test_home();

    let state = create_test_state().expect("create test state");

    let default = state
        .db
        .get_pricing_model_source("claude")
        .await
        .expect("read default pricing model source");
    assert_eq!(default, "response");

    state
        .db
        .set_pricing_model_source("claude", "request")
        .await
        .expect("set pricing model source");
    let updated = state
        .db
        .get_pricing_model_source("claude")
        .await
        .expect("read updated pricing model source");
    assert_eq!(updated, "request");

    let err = state
        .db
        .set_pricing_model_source("claude", "invalid")
        .await
        .expect_err("invalid pricing model source should error");
    // The error is a Localized variant (i18n-aware).
    match err {
        AppError::Localized { key, .. } => {
            assert_eq!(key, "error.invalidPricingMode");
        }
        other => panic!("expected localized error, got {other:?}"),
    }
}
