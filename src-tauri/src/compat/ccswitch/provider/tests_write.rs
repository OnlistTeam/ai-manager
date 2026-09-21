use super::{api_key_slots, app_type_for, apply_draft, slot_value};
use crate::domain::{ProviderDraft, ToolId};
use crate::provider::Provider as UpstreamProvider;
use serde_json::json;

fn upstream(id: &str, name: &str, settings: serde_json::Value) -> UpstreamProvider {
    UpstreamProvider::with_id(id.to_string(), name.to_string(), settings, None)
}

fn draft(name: &str, key: Option<&str>) -> ProviderDraft {
    ProviderDraft {
        name: name.to_string(),
        api_key: key.map(str::to_string),
        models: None,
        advanced: None,
    }
}

#[test]
fn a_written_key_is_read_back_by_the_upstream_reader_for_every_tool() {
    // This is the core guarantee of this task: the slot we write is exactly the slot upstream reads.
    let cases = [
        (
            ToolId::ClaudeCode,
            json!({ "env": { "ANTHROPIC_BASE_URL": "https://a.example.com",
                             "ANTHROPIC_AUTH_TOKEN": "sk-old-0123456789ABCD" } }),
        ),
        (
            ToolId::Codex,
            json!({ "auth": { "OPENAI_API_KEY": "sk-old-0123456789ABCD" } }),
        ),
        (
            ToolId::OpenCode,
            json!({ "options": { "baseURL": "https://o.example.com",
                                 "apiKey": "sk-old-0123456789ABCD" } }),
        ),
        (
            ToolId::GeminiCli,
            json!({ "env": { "GOOGLE_GEMINI_BASE_URL": "https://g.example.com",
                             "GEMINI_API_KEY": "sk-old-0123456789ABCD" } }),
        ),
        (
            ToolId::GrokBuild,
            json!({ "config": "[models]\ndefault = \"profile\"\n\n[model.profile]\nmodel = \"grok-4.5\"\nbase_url = \"https://api.x.ai/v1\"\nname = \"xAI\"\napi_key = \"sk-old-0123456789ABCD\"\napi_backend = \"responses\"\ncontext_window = 500000\n" }),
        ),
        (
            ToolId::OpenClaw,
            json!({ "baseUrl": "https://o.example.com", "apiKey": "sk-old-0123456789ABCD", "api": "openai-completions", "models": [{ "id": "model" }] }),
        ),
        (
            ToolId::Hermes,
            json!({ "base_url": "https://h.example.com", "api_key": "sk-old-0123456789ABCD", "api_mode": "chat_completions", "models": [{ "id": "model" }] }),
        ),
        (
            ToolId::Pi,
            json!({ "baseUrl": "https://p.example.com", "apiKey": "sk-old-0123456789ABCD", "api": "openai-completions", "models": [{ "id": "model" }] }),
        ),
    ];
    for (tool, settings) in cases {
        let mut raw = upstream("p", "Old name", settings);
        apply_draft(
            tool,
            &mut raw,
            &draft("New name", Some("sk-new-9876543210WXYZ")),
        )
        .expect("the draft is valid");

        let (_base, key) = raw.resolve_usage_credentials(&app_type_for(tool));
        assert_eq!(
            key, "sk-new-9876543210WXYZ",
            "{tool:?} key did not round trip"
        );
        assert_eq!(raw.name, "New name");
    }
}

#[test]
fn an_absent_key_leaves_the_stored_one_untouched() {
    let mut raw = upstream(
        "p",
        "Old name",
        json!({ "env": { "ANTHROPIC_BASE_URL": "https://a.example.com",
                         "ANTHROPIC_AUTH_TOKEN": "sk-keep-0123456789ABCD" } }),
    );
    apply_draft(ToolId::ClaudeCode, &mut raw, &draft("Renamed", None)).expect("valid");

    let (_base, key) = raw.resolve_usage_credentials(&app_type_for(ToolId::ClaudeCode));
    assert_eq!(key, "sk-keep-0123456789ABCD");
    assert_eq!(raw.name, "Renamed");
}

#[test]
fn writing_a_key_never_creates_a_second_competing_slot() {
    // For a service already using ANTHROPIC_API_KEY, the new key must stay in that slot and
    // must not open a second ANTHROPIC_AUTH_TOKEN that fights with it.
    let mut raw = upstream(
        "p",
        "P",
        json!({ "env": { "ANTHROPIC_BASE_URL": "https://a.example.com",
                         "ANTHROPIC_API_KEY": "sk-old-0123456789ABCD" } }),
    );
    apply_draft(
        ToolId::ClaudeCode,
        &mut raw,
        &draft("P", Some("sk-new-9876543210WXYZ")),
    )
    .expect("valid");

    let env = raw.settings_config["env"].clone();
    assert_eq!(env["ANTHROPIC_API_KEY"], json!("sk-new-9876543210WXYZ"));
    assert!(
        env.get("ANTHROPIC_AUTH_TOKEN").is_none(),
        "a second key slot was invented: {env}"
    );
}

/// Pin "overwrite the slot that is currently non-empty" down tool by tool and slot by slot:
/// whenever a slot already holds a key, the new key must land in that same slot and must never
/// open a second one — otherwise the two fight and what upstream reads back is not necessarily
/// the key just written.
#[test]
fn a_key_is_overwritten_wherever_it_already_lives_for_every_tool_and_slot() {
    for tool in ToolId::ALL {
        if !crate::compat::ccswitch::tools::capabilities_for(tool).can_manage_provider {
            continue;
        }
        for slot in api_key_slots(tool) {
            let mut section = serde_json::Map::new();
            section.insert(slot[1].to_string(), json!("sk-old-0123456789ABCD"));
            let mut root = serde_json::Map::new();
            root.insert(slot[0].to_string(), serde_json::Value::Object(section));
            let mut raw = upstream("p", "P", serde_json::Value::Object(root));

            apply_draft(tool, &mut raw, &draft("P", Some("sk-new-9876543210WXYZ"))).expect("valid");

            assert_eq!(
                raw.settings_config[slot[0]][slot[1]],
                json!("sk-new-9876543210WXYZ"),
                "{tool:?} did not overwrite {slot:?} in place"
            );
            let filled: Vec<_> = api_key_slots(tool)
                .iter()
                .filter(|other| slot_value(&raw.settings_config, other).is_some())
                .collect();
            assert_eq!(
                filled.len(),
                1,
                "{tool:?} left more than one key slot filled: {filled:?}"
            );
            let (_base, key) = raw.resolve_usage_credentials(&app_type_for(tool));
            assert_eq!(
                key, "sk-new-9876543210WXYZ",
                "{tool:?} slot {slot:?} did not round trip"
            );
        }
    }
}

/// The upstream Claude branch reads the key in ANTHROPIC_AUTH_TOKEN / ANTHROPIC_API_KEY /
/// OPENROUTER_API_KEY / GOOGLE_API_KEY order, and all four slots must be in `api_key_slots`:
/// leaving the last one out would open a separate ANTHROPIC_AUTH_TOKEN for a service that only
/// uses it, and the old key would stay put as a second one.
#[test]
fn a_claude_key_kept_in_the_google_fallback_slot_is_replaced_there_too() {
    let mut raw = upstream(
        "p",
        "P",
        json!({ "env": { "GOOGLE_API_KEY": "sk-old-0123456789ABCD" } }),
    );
    apply_draft(
        ToolId::ClaudeCode,
        &mut raw,
        &draft("P", Some("sk-new-9876543210WXYZ")),
    )
    .expect("valid");

    let env = raw.settings_config["env"].clone();
    assert_eq!(env["GOOGLE_API_KEY"], json!("sk-new-9876543210WXYZ"));
    assert!(
        env.get("ANTHROPIC_AUTH_TOKEN").is_none(),
        "a second key slot was invented: {env}"
    );
}

/// Important 1 regression: `slot_value` used to check emptiness after trimming, which is
/// stricter than the upstream bare `!s.is_empty()`. It treated a whitespace slot upstream
/// considers "non-empty" as empty, wrote the new key into the next slot, and upstream still read
/// back that whitespace slot. Claude and Gemini are both multi-slot tools, so both are pinned.
#[test]
fn a_slot_holding_only_whitespace_is_treated_as_filled_like_upstream_does() {
    let cases = [
        (
            ToolId::ClaudeCode,
            json!({ "env": { "ANTHROPIC_AUTH_TOKEN": " ",
                             "ANTHROPIC_API_KEY": "sk-old-0123456789ABCD" } }),
            "ANTHROPIC_AUTH_TOKEN",
        ),
        (
            ToolId::GeminiCli,
            json!({ "env": { "GEMINI_API_KEY": " ",
                             "GOOGLE_API_KEY": "sk-old-0123456789ABCD" } }),
            "GEMINI_API_KEY",
        ),
    ];
    for (tool, settings, whitespace_slot) in cases {
        let mut raw = upstream("p", "P", settings);
        apply_draft(tool, &mut raw, &draft("P", Some("sk-new-9876543210WXYZ"))).expect("valid");

        assert_eq!(
            raw.settings_config["env"][whitespace_slot],
            json!("sk-new-9876543210WXYZ"),
            "{tool:?} must overwrite the whitespace-occupied slot, not treat it as empty"
        );
        let (_base, key) = raw.resolve_usage_credentials(&app_type_for(tool));
        assert_eq!(
            key, "sk-new-9876543210WXYZ",
            "{tool:?} upstream reader must read back what we just wrote"
        );
    }
}

/// Minor 3: the existing oracle fills only one slot per case and therefore cannot detect slot
/// ordering (swapping Gemini's two slots stays green). Here both slots hold a real old key, which
/// pins the coupling between "write the first non-empty slot" and the upstream read order.
#[test]
fn a_multi_slot_tool_writes_the_first_slot_when_both_are_genuinely_filled() {
    let cases = [
        (
            ToolId::ClaudeCode,
            json!({ "env": { "ANTHROPIC_AUTH_TOKEN": "sk-old-first-0123456789AB",
                             "ANTHROPIC_API_KEY": "sk-old-second-0123456789AB" } }),
            "ANTHROPIC_AUTH_TOKEN",
            "ANTHROPIC_API_KEY",
        ),
        (
            ToolId::GeminiCli,
            json!({ "env": { "GEMINI_API_KEY": "sk-old-first-0123456789AB",
                             "GOOGLE_API_KEY": "sk-old-second-0123456789AB" } }),
            "GEMINI_API_KEY",
            "GOOGLE_API_KEY",
        ),
    ];
    for (tool, settings, first_slot, second_slot) in cases {
        let mut raw = upstream("p", "P", settings);
        apply_draft(tool, &mut raw, &draft("P", Some("sk-new-9876543210WXYZ"))).expect("valid");

        assert_eq!(
            raw.settings_config["env"][first_slot],
            json!("sk-new-9876543210WXYZ"),
            "{tool:?} must land the new key in the first slot"
        );
        assert_eq!(
            raw.settings_config["env"][second_slot],
            json!("sk-old-second-0123456789AB"),
            "{tool:?} must leave the second slot untouched"
        );
        let (_base, key) = raw.resolve_usage_credentials(&app_type_for(tool));
        assert_eq!(
            key, "sk-new-9876543210WXYZ",
            "{tool:?} upstream reader must read back the first slot"
        );
    }
}

#[test]
fn a_key_on_a_service_that_had_none_lands_in_the_first_slot() {
    let mut raw = upstream(ToolId::ClaudeCode.as_str(), "P", json!({ "env": {} }));
    apply_draft(
        ToolId::ClaudeCode,
        &mut raw,
        &draft("P", Some("sk-new-9876543210WXYZ")),
    )
    .expect("valid");
    assert_eq!(
        raw.settings_config["env"]["ANTHROPIC_AUTH_TOKEN"],
        json!("sk-new-9876543210WXYZ")
    );
}

#[test]
fn writing_only_touches_the_two_fields_the_product_owns() {
    let mut raw = upstream(
        "p",
        "P",
        json!({
            "env": { "ANTHROPIC_BASE_URL": "https://a.example.com",
                     "ANTHROPIC_AUTH_TOKEN": "sk-old-0123456789ABCD",
                     "SOMETHING_ELSE": "keep me" },
            "permissions": { "allow": ["Bash"] }
        }),
    );
    raw.notes = Some("hand written note".to_string());
    apply_draft(
        ToolId::ClaudeCode,
        &mut raw,
        &draft("P", Some("sk-new-9876543210WXYZ")),
    )
    .expect("valid");

    assert_eq!(
        raw.settings_config["env"]["SOMETHING_ELSE"],
        json!("keep me")
    );
    assert_eq!(
        raw.settings_config["permissions"]["allow"][0],
        json!("Bash")
    );
    assert_eq!(raw.notes.as_deref(), Some("hand written note"));
    assert_eq!(
        raw.settings_config["env"]["ANTHROPIC_BASE_URL"],
        json!("https://a.example.com"),
        "the address is read-only in this phase and must survive untouched"
    );
}

#[test]
fn a_blank_name_is_refused_before_anything_is_written() {
    let mut raw = upstream(
        "p",
        "Original",
        json!({ "env": { "ANTHROPIC_AUTH_TOKEN": "sk-old-0123456789ABCD" } }),
    );
    let error = apply_draft(ToolId::ClaudeCode, &mut raw, &draft("   ", None))
        .expect_err("a blank name is not a name");
    assert_eq!(error.message_key, "error.provider.nameRequired");
    assert_eq!(
        raw.name, "Original",
        "nothing may be written on the refusal path"
    );
}

#[test]
fn a_blank_replacement_key_is_refused_rather_than_wiping_the_stored_one() {
    // "leave empty = unchanged" is apiKey: null on the wire; apiKey: "" means the user really
    // cleared the box, which is neither "unchanged" nor a valid key — reject it rather than
    // silently wiping the stored one.
    let mut raw = upstream(
        "p",
        "P",
        json!({ "env": { "ANTHROPIC_AUTH_TOKEN": "sk-old-0123456789ABCD" } }),
    );
    let error = apply_draft(ToolId::ClaudeCode, &mut raw, &draft("P", Some("  ")))
        .expect_err("a blank key is refused");
    assert_eq!(error.message_key, "error.provider.saveFailed");
    assert_eq!(
        error.remediation.as_deref(),
        Some("error.remediation.checkServiceSettings")
    );

    let (_base, key) = raw.resolve_usage_credentials(&app_type_for(ToolId::ClaudeCode));
    assert_eq!(
        key, "sk-old-0123456789ABCD",
        "the stored key must survive a refused write"
    );
}

/// Important 2 regression: `write_slot` used to silently coerce a corrupted `settings_config`
/// into an empty object before writing — destroying the user's hand-written fields, addresses and
/// permissions and persisting that through save; upstream's own `validate_provider_settings` could
/// not catch it afterwards either, because it would only see the empty object we just created.
/// Corrupted rows really are reachable through the dao's `unwrap_or(Null)`. It must now be
/// rejected before writing, leaving the original value byte for byte, and both the top level and
/// the inner section must be rejected (covering both checks in write_slot).
#[test]
fn a_non_object_settings_config_is_refused_before_anything_is_written() {
    let corrupted = [
        json!("not an object"),
        serde_json::Value::Null,
        json!({ "env": "not an object either" }),
    ];
    for settings in corrupted {
        let mut raw = upstream("p", "Original", settings.clone());
        let error = apply_draft(
            ToolId::ClaudeCode,
            &mut raw,
            &draft("New name", Some("sk-new-9876543210WXYZ")),
        )
        .expect_err(&format!("{settings} must be refused"));
        assert_eq!(error.message_key, "error.provider.saveFailed");
        assert_eq!(
            error.remediation.as_deref(),
            Some("error.remediation.checkServiceSettings")
        );
        assert_eq!(
            raw.settings_config, settings,
            "settings_config must survive a refused write untouched"
        );
        assert_eq!(
            raw.name, "Original",
            "the name must not be written either on the refusal path"
        );
    }
}
