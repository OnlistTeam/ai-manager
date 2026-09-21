use std::collections::HashMap;

use serde_json::json;

use super::{advanced, apply_draft};
use crate::domain::{ProviderAdvancedDraft, ProviderDraft, ProviderHeaderDraft, ToolId};
use crate::provider::Provider as UpstreamProvider;
use crate::provider::ProviderMeta;
use crate::settings::CustomEndpoint;

const KEY: &str = "sk-provider-secret-0123456789ABCD";
const HEADER_SECRET: &str = "Bearer header-secret-0123456789ABCD";

fn provider(settings: serde_json::Value) -> UpstreamProvider {
    UpstreamProvider::with_id("relay".to_string(), "Relay".to_string(), settings, None)
}

fn draft(
    models: Option<Vec<&str>>,
    base_url: Option<&str>,
    headers: Option<Vec<ProviderHeaderDraft>>,
) -> ProviderDraft {
    ProviderDraft {
        name: "Relay".to_string(),
        api_key: None,
        models: models.map(|models| models.into_iter().map(str::to_string).collect()),
        advanced: Some(ProviderAdvancedDraft {
            base_url_changed: true,
            base_url: base_url.map(str::to_string),
            endpoint_candidates: None,
            endpoint_auto_select: None,
            headers,
        }),
    }
}

#[test]
fn safe_profiles_project_each_tools_native_endpoint_and_model_shape() {
    let cases = [
        (
            ToolId::ClaudeCode,
            provider(json!({
                "env": {
                    "ANTHROPIC_BASE_URL": "https://claude.example/v1",
                    "ANTHROPIC_AUTH_TOKEN": KEY,
                    "ANTHROPIC_MODEL": "claude-model"
                }
            })),
            "https://claude.example/v1",
            vec!["claude-model"],
        ),
        (
            ToolId::Codex,
            provider(json!({
                "auth": { "OPENAI_API_KEY": KEY },
                "config": "model_provider = \"custom\"\nmodel = \"codex-model\"\n[model_providers.custom]\nbase_url = \"https://codex.example/v1\"\n"
            })),
            "https://codex.example/v1",
            vec!["codex-model"],
        ),
        (
            ToolId::GeminiCli,
            provider(json!({
                "env": {
                    "GOOGLE_GEMINI_BASE_URL": "https://gemini.example/v1",
                    "GEMINI_API_KEY": KEY,
                    "GEMINI_MODEL": "gemini-model"
                }
            })),
            "https://gemini.example/v1",
            vec!["gemini-model"],
        ),
        (
            ToolId::OpenCode,
            provider(json!({
                "options": { "baseURL": "https://opencode.example/v1", "apiKey": KEY },
                "models": { "z-model": { "name": "Z" }, "a-model": { "name": "A" } }
            })),
            "https://opencode.example/v1",
            vec!["a-model", "z-model"],
        ),
        (
            ToolId::GrokBuild,
            provider(json!({
                "config": "[models]\ndefault = \"profile\"\n\n[model.profile]\nmodel = \"grok-model\"\nbase_url = \"https://grok.example/v1\"\nname = \"Grok\"\napi_key = \"sk-provider-secret-0123456789ABCD\"\napi_backend = \"responses\"\ncontext_window = 500000\n"
            })),
            "https://grok.example/v1",
            vec!["grok-model"],
        ),
        (
            ToolId::OpenClaw,
            provider(json!({
                "baseUrl": "https://openclaw.example/v1",
                "apiKey": KEY,
                "api": "openai-completions",
                "models": [{ "id": "openclaw-model" }]
            })),
            "https://openclaw.example/v1",
            vec!["openclaw-model"],
        ),
        (
            ToolId::Hermes,
            provider(json!({
                "base_url": "https://hermes.example/v1",
                "api_key": KEY,
                "api_mode": "chat_completions",
                "models": [{ "id": "hermes-model" }]
            })),
            "https://hermes.example/v1",
            vec!["hermes-model"],
        ),
        (
            ToolId::Pi,
            provider(json!({
                "baseUrl": "https://pi.example/v1",
                "apiKey": KEY,
                "api": "openai-completions",
                "models": [{ "id": "pi-model" }]
            })),
            "https://pi.example/v1",
            vec!["pi-model"],
        ),
    ];

    for (tool, raw, base_url, models) in cases {
        let profile = advanced::profile(tool, &raw);
        assert_eq!(profile.base_url.as_deref(), Some(base_url), "{tool:?}");
        assert_eq!(profile.models, models, "{tool:?}");
        assert!(profile.capabilities.can_edit_base_url, "{tool:?}");
        assert!(profile.capabilities.can_edit_models, "{tool:?}");

        let wire = serde_json::to_string(&profile).expect("serialize safe profile");
        assert!(!wire.contains(KEY), "{tool:?} profile exposed a key");
        assert!(!wire.contains("settingsConfig"));
    }
}

#[test]
fn common_endpoint_and_model_edits_round_trip_for_every_product_tool() {
    let cases = [
        (
            ToolId::ClaudeCode,
            provider(json!({ "env": { "ANTHROPIC_AUTH_TOKEN": KEY }, "keep": 1 })),
        ),
        (
            ToolId::Codex,
            provider(json!({
                "auth": { "OPENAI_API_KEY": KEY },
                "config": "model_provider = \"custom\"\n[model_providers.custom]\nwire_api = \"responses\"\n",
                "keep": 1
            })),
        ),
        (
            ToolId::GeminiCli,
            provider(json!({ "env": { "GEMINI_API_KEY": KEY }, "config": {}, "keep": 1 })),
        ),
        (
            ToolId::OpenCode,
            provider(json!({
                "npm": "@ai-sdk/openai-compatible",
                "options": { "apiKey": KEY, "custom": true },
                "models": {},
                "keep": 1
            })),
        ),
        (
            ToolId::GrokBuild,
            provider(json!({
                "config": "[models]\ndefault = \"profile\"\n\n[model.profile]\nmodel = \"old-model\"\nbase_url = \"https://old.example/v1\"\nname = \"Grok\"\napi_key = \"sk-provider-secret-0123456789ABCD\"\napi_backend = \"responses\"\ncontext_window = 500000\n",
                "keep": 1
            })),
        ),
        (
            ToolId::OpenClaw,
            provider(json!({
                "baseUrl": "https://old.example/v1", "apiKey": KEY,
                "api": "openai-completions", "models": [{ "id": "old", "keep": true }],
                "keep": 1
            })),
        ),
        (
            ToolId::Hermes,
            provider(json!({
                "base_url": "https://old.example/v1", "api_key": KEY,
                "api_mode": "chat_completions", "models": [{ "id": "old", "keep": true }],
                "keep": 1
            })),
        ),
        (
            ToolId::Pi,
            provider(json!({
                "baseUrl": "https://old.example/v1", "apiKey": KEY,
                "api": "openai-completions", "models": [{ "id": "old", "keep": true }],
                "keep": 1
            })),
        ),
    ];

    for (tool, mut raw) in cases {
        let endpoint = format!("https://{}.example/v1", tool.as_str());
        let model = format!("{}-model", tool.as_str());
        apply_draft(
            tool,
            &mut raw,
            &draft(Some(vec![&model]), Some(&endpoint), None),
        )
        .expect("advanced edit");

        let profile = advanced::profile(tool, &raw);
        assert_eq!(
            profile.base_url.as_deref(),
            Some(endpoint.as_str()),
            "{tool:?}"
        );
        assert_eq!(profile.models, vec![model], "{tool:?}");
        assert_eq!(raw.settings_config["keep"], json!(1), "{tool:?}");
        let (_, key) = raw.resolve_usage_credentials(&super::app_type_for(tool));
        assert_eq!(key, KEY, "{tool:?} key must be preserved");
    }
}

#[test]
fn alternate_routes_project_and_update_with_the_common_provider_draft() {
    let mut raw = provider(json!({
        "env": {
            "ANTHROPIC_BASE_URL": "https://primary.example/v1",
            "ANTHROPIC_AUTH_TOKEN": KEY
        }
    }));
    raw.meta = Some(ProviderMeta {
        custom_endpoints: HashMap::from([
            (
                "https://later.example/v1".to_string(),
                CustomEndpoint {
                    url: "https://later.example/v1".to_string(),
                    added_at: 20,
                    last_used: None,
                },
            ),
            (
                "https://first.example/v1".to_string(),
                CustomEndpoint {
                    url: "https://first.example/v1".to_string(),
                    added_at: 10,
                    last_used: Some(12),
                },
            ),
        ]),
        endpoint_auto_select: Some(true),
        ..Default::default()
    });

    let profile = advanced::profile(ToolId::ClaudeCode, &raw);
    assert_eq!(
        profile.endpoint_candidates,
        vec![
            "https://first.example/v1".to_string(),
            "https://later.example/v1".to_string()
        ]
    );
    assert!(profile.endpoint_auto_select);
    assert!(profile.capabilities.can_edit_endpoints);

    apply_draft(
        ToolId::ClaudeCode,
        &mut raw,
        &ProviderDraft {
            name: "Relay".to_string(),
            api_key: None,
            models: None,
            advanced: Some(ProviderAdvancedDraft {
                base_url_changed: true,
                base_url: Some("https://later.example/v1".to_string()),
                endpoint_candidates: Some(vec![
                    "https://primary.example/v1".to_string(),
                    "https://later.example/v1".to_string(),
                ]),
                endpoint_auto_select: Some(false),
                headers: None,
            }),
        },
    )
    .expect("update routes through the typed draft");

    let profile = advanced::profile(ToolId::ClaudeCode, &raw);
    assert_eq!(
        profile.base_url.as_deref(),
        Some("https://later.example/v1")
    );
    assert_eq!(
        profile.endpoint_candidates,
        vec![
            "https://later.example/v1".to_string(),
            "https://primary.example/v1".to_string()
        ]
    );
    assert!(!profile.endpoint_auto_select);
    assert_eq!(
        raw.meta
            .as_ref()
            .and_then(|meta| meta.custom_endpoints.get("https://later.example/v1"))
            .map(|endpoint| endpoint.added_at),
        Some(20),
        "an unchanged route keeps its original timestamp"
    );
}

#[test]
fn unsafe_legacy_routes_fail_closed_without_exposing_them() {
    let mut raw = provider(json!({
        "env": {
            "ANTHROPIC_BASE_URL": "https://primary.example/v1",
            "ANTHROPIC_AUTH_TOKEN": KEY
        }
    }));
    raw.meta = Some(ProviderMeta {
        custom_endpoints: HashMap::from([(
            "unsafe".to_string(),
            CustomEndpoint {
                url: "https://user:secret@example.test/v1?token=hidden".to_string(),
                added_at: 1,
                last_used: None,
            },
        )]),
        ..Default::default()
    });

    let profile = advanced::profile(ToolId::ClaudeCode, &raw);
    assert!(profile.endpoint_candidates.is_empty());
    assert!(!profile.capabilities.can_edit_endpoints);
    assert!(profile.capabilities.can_edit_base_url);
    let wire = serde_json::to_string(&profile).expect("serialize safe profile");
    assert!(!wire.contains("secret"));
    assert!(!wire.contains("hidden"));

    let error = apply_draft(
        ToolId::ClaudeCode,
        &mut raw,
        &ProviderDraft {
            name: "Relay".to_string(),
            api_key: None,
            models: None,
            advanced: Some(ProviderAdvancedDraft {
                base_url_changed: false,
                base_url: None,
                endpoint_candidates: Some(vec!["https://safe.example/v1".to_string()]),
                endpoint_auto_select: None,
                headers: None,
            }),
        },
    )
    .expect_err("unsafe hidden inventory must disable route mutation");
    assert_eq!(error.message_key, "error.provider.saveFailed");
}

#[test]
fn opencode_headers_are_write_only_and_model_metadata_is_preserved() {
    let mut raw = provider(json!({
        "npm": "@ai-sdk/openai-compatible",
        "options": {
            "baseURL": "https://old.example/v1",
            "apiKey": KEY,
            "headers": {
                "Authorization": HEADER_SECRET,
                "X-Remove": "remove-me"
            },
            "custom": "keep"
        },
        "models": {
            "keep-model": { "name": "Keep", "limit": { "context": 12345 } },
            "remove-model": { "name": "Remove" }
        }
    }));
    let profile = advanced::profile(ToolId::OpenCode, &raw);
    let wire = serde_json::to_string(&profile).expect("serialize profile");
    assert_eq!(profile.header_names, vec!["Authorization", "X-Remove"]);
    assert!(!wire.contains(HEADER_SECRET));

    apply_draft(
        ToolId::OpenCode,
        &mut raw,
        &draft(
            Some(vec!["keep-model", "new-model"]),
            Some("https://new.example/v1"),
            Some(vec![
                ProviderHeaderDraft {
                    name: "Authorization".to_string(),
                    value: None,
                },
                ProviderHeaderDraft {
                    name: "X-Trace".to_string(),
                    value: Some("trace-123".to_string()),
                },
            ]),
        ),
    )
    .expect("edit OpenCode settings");

    assert_eq!(
        raw.settings_config["options"]["headers"]["Authorization"],
        json!(HEADER_SECRET)
    );
    assert_eq!(
        raw.settings_config["options"]["headers"]["X-Trace"],
        json!("trace-123")
    );
    assert!(raw.settings_config["options"]["headers"]
        .get("X-Remove")
        .is_none());
    assert_eq!(raw.settings_config["options"]["custom"], json!("keep"));
    assert_eq!(
        raw.settings_config["models"]["keep-model"]["limit"]["context"],
        json!(12345)
    );
    assert_eq!(
        raw.settings_config["models"]["new-model"]["name"],
        json!("new-model")
    );
    assert!(raw.settings_config["models"].get("remove-model").is_none());
}

#[test]
fn additive_tool_model_edits_preserve_matching_native_metadata() {
    for (tool, settings) in [
        (
            ToolId::OpenClaw,
            json!({
                "baseUrl": "https://example.test/v1", "apiKey": KEY,
                "models": [{ "id": "keep", "contextWindow": 12345 }, { "id": "remove" }]
            }),
        ),
        (
            ToolId::Hermes,
            json!({
                "base_url": "https://example.test/v1", "api_key": KEY,
                "models": [{ "id": "keep", "context_length": 12345 }, { "id": "remove" }]
            }),
        ),
        (
            ToolId::Pi,
            json!({
                "baseUrl": "https://example.test/v1", "apiKey": KEY,
                "models": [{ "id": "keep", "contextWindow": 12345 }, { "id": "remove" }]
            }),
        ),
    ] {
        let mut raw = provider(settings);
        apply_draft(
            tool,
            &mut raw,
            &draft(
                Some(vec!["keep", "new"]),
                Some("https://example.test/v1"),
                None,
            ),
        )
        .expect("native array edit");

        assert_eq!(raw.settings_config["models"][0]["id"], "keep");
        assert_eq!(raw.settings_config["models"][1]["id"], "new");
        assert!(
            raw.settings_config["models"][0]
                .as_object()
                .expect("model object")
                .len()
                > 1
        );
    }
}

#[test]
fn hermes_dict_overlay_entries_remain_read_only() {
    let raw = provider(json!({
        "_cc_source": "providers_dict",
        "base_url": "https://example.test/v1",
        "api_key": KEY,
        "models": [{ "id": "model" }]
    }));
    let profile = advanced::profile(ToolId::Hermes, &raw);
    assert!(!profile.capabilities.can_edit_base_url);
    assert!(!profile.capabilities.can_edit_models);
}

#[test]
fn unsafe_endpoint_or_header_input_is_rejected_without_partial_mutation() {
    let original = provider(json!({
        "options": {
            "baseURL": "http://legacy.remote.example/v1",
            "apiKey": KEY,
            "headers": { "Authorization": HEADER_SECRET }
        },
        "models": { "old": { "name": "Old" } }
    }));

    let mut unchanged_legacy = original.clone();
    apply_draft(
        ToolId::OpenCode,
        &mut unchanged_legacy,
        &draft(
            Some(vec!["old"]),
            Some("http://legacy.remote.example/v1"),
            Some(vec![ProviderHeaderDraft {
                name: "Authorization".to_string(),
                value: None,
            }]),
        ),
    )
    .expect("an unchanged legacy endpoint remains preservable");

    let invalid = [
        draft(None, Some("http://remote.example/v1"), None),
        draft(
            None,
            Some("https://safe.example/v1"),
            Some(vec![ProviderHeaderDraft {
                name: "X-Injected".to_string(),
                value: Some("ok\r\nInjected: bad".to_string()),
            }]),
        ),
    ];
    for bad in invalid {
        let mut candidate = original.clone();
        let error = apply_draft(ToolId::OpenCode, &mut candidate, &bad)
            .expect_err("unsafe input must be rejected");
        assert_eq!(error.message_key, "error.provider.saveFailed");
        assert_eq!(
            serde_json::to_value(&candidate).expect("serialize candidate"),
            serde_json::to_value(&original).expect("serialize original"),
            "the in-memory snapshot changed on a refusal path"
        );
    }

    let mut localhost = original;
    apply_draft(
        ToolId::OpenCode,
        &mut localhost,
        &draft(None, Some("http://127.0.0.1:8080/v1"), None),
    )
    .expect("loopback HTTP is allowed");
}

#[test]
fn managed_services_fail_closed_and_header_debug_output_is_redacted() {
    let mut raw = provider(json!({
        "options": { "baseURL": "https://example.test", "apiKey": KEY },
        "models": {}
    }));
    raw.category = Some("omo".to_string());
    let profile = advanced::profile(ToolId::OpenCode, &raw);
    assert!(!profile.capabilities.can_edit_base_url);
    assert!(!profile.capabilities.can_edit_models);
    assert!(!profile.capabilities.can_edit_headers);
    assert!(apply_draft(
        ToolId::OpenCode,
        &mut raw,
        &draft(None, Some("https://changed.example"), None)
    )
    .is_err());

    let header = ProviderHeaderDraft {
        name: "Authorization".to_string(),
        value: Some(HEADER_SECRET.to_string()),
    };
    let debug = format!("{header:?}");
    assert!(!debug.contains(HEADER_SECRET));
    assert!(debug.contains("***"));
}

#[test]
fn edit_profiles_do_not_return_credentialed_or_query_token_urls() {
    for base_url in [
        "https://user:password@example.test/v1",
        "https://example.test/v1?token=secret-query-value",
    ] {
        let raw = provider(json!({
            "options": { "baseURL": base_url, "apiKey": KEY },
            "models": {}
        }));
        let profile = advanced::profile(ToolId::OpenCode, &raw);
        assert_eq!(profile.base_url, None);
        assert!(profile.endpoint_candidates.is_empty());
        assert!(!profile.capabilities.can_edit_endpoints);
        let wire = serde_json::to_string(&profile).expect("serialize safe profile");
        assert!(!wire.contains("password"));
        assert!(!wire.contains("secret-query-value"));
    }
}
