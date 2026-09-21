use std::collections::BTreeMap;

use serde_json::json;

use super::*;
use crate::platform::{ShellEnvironment, ShellEnvironmentSource};

fn env(entries: &[(&str, &str)]) -> ToolEnvironment {
    ToolEnvironment::from_parts(
        ShellEnvironment::new(
            entries
                .iter()
                .map(|(key, value)| (key.to_string(), value.to_string()))
                .collect::<BTreeMap<_, _>>(),
            ShellEnvironmentSource::ProcessFallback,
        ),
        vec![],
    )
}

fn resolve(config: Value, history: Value, auth: Option<Value>) -> Resolution {
    resolve_with_state(
        Path::new("opencode.jsonc"),
        &config,
        Some(&history),
        Path::new("auth.json"),
        auth.as_ref(),
        &env(&[]),
    )
}

#[test]
fn recent_model_and_auth_store_are_evidence_even_without_global_model() {
    let result = resolve(
        json!({"provider":{"relay":{"options":{"baseURL":"https://relay.example/v1"}}}}),
        json!({"recent":[{"providerID":"relay","modelID":"openai/model"}]}),
        Some(json!({"relay":{"type":"api","key":"test-private-value"}})),
    );
    assert_eq!(result.selection, EffectiveSelection::RecentModel);
    assert_eq!(result.model.as_deref(), Some("relay/openai/model"));
    assert_eq!(result.provider_hint.as_deref(), Some("relay"));
    assert_eq!(result.endpoint.as_deref(), Some("https://relay.example/v1"));
    assert_eq!(result.credential, EffectiveCredential::Configured);
    assert_eq!(result.credential_source, live(Path::new("auth.json")));
    assert!(!format!("{result:?}").contains("test-private-value"));
}

#[test]
fn explicit_default_beats_history_even_when_history_is_unreadable() {
    let result = resolve(
        json!({"model":"default/model"}),
        json!({"recent":[{"providerID":"old","modelID":"model"}]}),
        None,
    );
    assert_eq!(result.selection, EffectiveSelection::DefaultModel);
    assert_eq!(result.provider_hint.as_deref(), Some("default"));
    let result = resolve_with_state(
        Path::new("config"),
        &json!({"model":"default/model"}),
        None,
        Path::new("auth"),
        None,
        &env(&[]),
    );
    assert_eq!(result.selection, EffectiveSelection::DefaultModel);
    assert_eq!(result.credential, EffectiveCredential::Unknown);
}

#[test]
fn disabled_recent_is_skipped_and_disabled_default_is_unknown() {
    let result = resolve(
        json!({"disabled_providers":["disabled"]}),
        json!({"recent":[{"providerID":"disabled","modelID":"m"},{"providerID":"allowed","modelID":"m"}]}),
        None,
    );
    assert_eq!(result.provider_hint.as_deref(), Some("allowed"));
    let result = resolve(
        json!({"enabled_providers":["other"],"model":"disabled/m"}),
        json!({}),
        None,
    );
    assert_eq!(result.selection, EffectiveSelection::Unknown);
}

#[test]
fn unknown_builtin_history_is_not_replaced_by_older_custom_history() {
    let result = resolve(
        json!({"provider":{"custom":{}}}),
        json!({"recent":[{"providerID":"builtin","modelID":"m"},{"providerID":"custom","modelID":"m"}]}),
        Some(json!({"builtin":{"type":"oauth","refresh":"private-refresh"}})),
    );
    assert_eq!(result.provider_hint.as_deref(), Some("builtin"));
    assert_eq!(result.endpoint, None);
    assert_eq!(result.credential, EffectiveCredential::ToolLogin);
}

#[test]
fn lone_provider_and_invalid_models_are_never_guessed_active() {
    for config in [
        json!({"provider":{"only":{}}}),
        json!({"model":"only"}),
        json!({"model":"only/"}),
        json!({"model":42}),
        json!({"model":"only/model\nsecret"}),
    ] {
        let result = resolve(config, json!({}), None);
        assert_eq!(result.selection, EffectiveSelection::Unknown);
        assert_eq!(result.provider_hint, None);
    }
}

#[test]
fn credential_templates_are_not_mistaken_for_keys() {
    for key in ["{env:MISSING}", "{file:/some/key}", ""] {
        let result = resolve(
            json!({"model":"relay/m","provider":{"relay":{"options":{"apiKey":key}}}}),
            json!({}),
            Some(json!({"relay":{"type":"api","key":"old"}})),
        );
        assert_eq!(result.credential, EffectiveCredential::Unknown);
    }
}

#[test]
fn environment_templates_resolve_without_exposing_values() {
    let result = resolve_with_state(
        Path::new("config"),
        &json!({"model":"relay/m","provider":{"relay":{"options":{"baseURL":"{env:URL}","apiKey":"{env:KEY}"}}}}),
        None,
        Path::new("auth"),
        None,
        &env(&[("URL", "https://relay.example"), ("KEY", "private-value")]),
    );
    assert_eq!(result.endpoint.as_deref(), Some("https://relay.example/"));
    assert_eq!(result.credential, EffectiveCredential::Configured);
    assert!(!format!("{result:?}").contains("private-value"));
}

#[test]
fn malformed_auth_and_empty_keys_are_unknown_not_missing() {
    for auth in [
        None,
        Some(json!({"relay":{"type":"api","key":" "}})),
        Some(json!({"relay":{"type":"unexpected","key":"value"}})),
    ] {
        assert_eq!(
            resolve(json!({"model":"relay/m"}), json!({}), auth).credential,
            EffectiveCredential::Unknown
        );
    }
}

#[test]
fn reads_are_bounded_and_missing_is_distinct_from_corrupt() {
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("state.json");
    assert_eq!(read_optional_json(&path), Ok(json!({})));
    for bytes in [
        b"[1]".to_vec(),
        b"{bad".to_vec(),
        vec![b' '; MAX_FILE_BYTES as usize + 1],
    ] {
        std::fs::write(&path, bytes).unwrap();
        assert!(read_optional_json(&path).is_err());
    }
    assert!(read_optional_json(temp.path()).is_err());
}

#[test]
fn custom_state_home_is_honored() {
    // Only an absolute `XDG_STATE_HOME` is honoured, and absoluteness is platform
    // specific: a bare `/tmp/...` carries no drive prefix and is relative on Windows.
    let custom_root = if cfg!(target_os = "windows") {
        r"C:\synthetic-state"
    } else {
        "/tmp/synthetic-state"
    };
    assert_eq!(
        state_path(&env(&[("XDG_STATE_HOME", custom_root)])),
        PathBuf::from(custom_root)
            .join("opencode")
            .join("model.json")
    );
}

#[test]
fn alternate_config_cannot_claim_standard_config_is_active() {
    for key in [
        "OPENCODE_CONFIG",
        "OPENCODE_CONFIG_CONTENT",
        "OPENCODE_CONFIG_DIR",
    ] {
        assert_eq!(
            resolve_live(&env(&[(key, "custom")])).selection,
            EffectiveSelection::Unknown
        );
    }
}
