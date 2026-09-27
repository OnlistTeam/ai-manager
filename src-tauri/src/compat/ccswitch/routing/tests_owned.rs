use serde_json::json;

use super::{owned_settings, restore_own_settings, LivePart};

const DOCUMENT: &[LivePart] = &[LivePart::Document];
const TOML_WITH_LOGIN: &[LivePart] = &[LivePart::Toml("config"), LivePart::AsBackedUp("auth")];

fn markers() -> Vec<String> {
    vec!["PROXY_MANAGED".to_string(), "127.0.0.1:15721".to_string()]
}

#[test]
fn only_the_routes_own_settings_go_back_and_later_changes_stay() {
    let backup = json!({
        "env": {
            "ANTHROPIC_BASE_URL": "https://relay.example.com",
            "ANTHROPIC_AUTH_TOKEN": "sk-relay",
            "ANTHROPIC_MODEL": "relay-model"
        },
        "permissions": { "allow": ["Read"] }
    });
    let routed = json!({
        "env": {
            "ANTHROPIC_BASE_URL": "http://127.0.0.1:15721",
            "ANTHROPIC_AUTH_TOKEN": "PROXY_MANAGED",
            "ANTHROPIC_DEFAULT_SONNET_MODEL": "claude-sonnet-5"
        },
        "permissions": { "allow": ["Read"] }
    });
    let owned = owned_settings(&backup, &routed, DOCUMENT);
    assert_eq!(owned.len(), 4, "{owned:?}");

    // While routed, the user allowed another tool and picked a model.
    let mut current = routed.clone();
    current["permissions"]["allow"] = json!(["Read", "Bash"]);
    current["model"] = json!("opus");

    let restored = restore_own_settings(&backup, &current, &owned, DOCUMENT, &markers());
    assert_eq!(
        restored,
        json!({
            "env": {
                "ANTHROPIC_BASE_URL": "https://relay.example.com",
                "ANTHROPIC_AUTH_TOKEN": "sk-relay",
                "ANTHROPIC_MODEL": "relay-model"
            },
            "permissions": { "allow": ["Read", "Bash"] },
            "model": "opus"
        })
    );
}

#[test]
fn a_route_setting_changed_by_someone_else_stays_unless_it_names_the_route() {
    let backup = json!({ "env": { "ANTHROPIC_BASE_URL": "https://relay.example.com" } });
    let routed = json!({
        "env": {
            "ANTHROPIC_BASE_URL": "http://127.0.0.1:15721",
            "ANTHROPIC_DEFAULT_OPUS_MODEL": "claude-opus-5"
        }
    });
    let owned = owned_settings(&backup, &routed, DOCUMENT);
    let current = json!({
        "env": {
            "ANTHROPIC_BASE_URL": "http://127.0.0.1:15721/v1",
            "ANTHROPIC_DEFAULT_OPUS_MODEL": "my-own-model",
            "EXTRA_URL": "http://127.0.0.1:15721/extra"
        }
    });

    let restored = restore_own_settings(&backup, &current, &owned, DOCUMENT, &markers());
    assert_eq!(
        restored,
        json!({
            "env": {
                "ANTHROPIC_BASE_URL": "https://relay.example.com",
                "ANTHROPIC_DEFAULT_OPUS_MODEL": "my-own-model"
            }
        })
    );
}

#[test]
fn a_section_the_route_created_is_removed_when_it_empties() {
    let backup = json!({ "theme": "dark" });
    let routed = json!({
        "theme": "dark",
        "env": { "GOOGLE_GEMINI_BASE_URL": "http://127.0.0.1:15721" }
    });
    let owned = owned_settings(&backup, &routed, DOCUMENT);
    let restored = restore_own_settings(&backup, &routed, &owned, DOCUMENT, &markers());
    assert_eq!(restored, json!({ "theme": "dark" }));

    // A section the tool already had stays, even when it ends up empty.
    let backup = json!({ "env": {} });
    let routed = json!({ "env": { "GEMINI_API_KEY": "PROXY_MANAGED" } });
    let owned = owned_settings(&backup, &routed, DOCUMENT);
    let restored = restore_own_settings(&backup, &routed, &owned, DOCUMENT, &markers());
    assert_eq!(restored, json!({ "env": {} }));
}

#[test]
fn toml_settings_are_put_back_in_the_current_file_keeping_its_other_edits() {
    let backup = json!({
        "auth": { "OPENAI_API_KEY": "sk-before" },
        "config": "model_provider = \"relay\"\n\n[model_providers.relay]\nbase_url = \"https://relay.example.com/v1\"\n"
    });
    let routed = json!({
        "auth": { "OPENAI_API_KEY": "sk-before" },
        "config": "model_provider = \"relay\"\n\n[model_providers.relay]\nbase_url = \"http://127.0.0.1:15721/v1\"\nexperimental_bearer_token = \"PROXY_MANAGED\"\n"
    });
    let owned = owned_settings(&backup, &routed, TOML_WITH_LOGIN);
    assert_eq!(owned.len(), 2, "{owned:?}");

    let current = json!({
        "auth": { "OPENAI_API_KEY": "sk-rotated-during-route" },
        "config": "# my servers\nmodel_provider = \"relay\"\n\n[model_providers.relay]\nbase_url = \"http://127.0.0.1:15721/v1\"\nexperimental_bearer_token = \"PROXY_MANAGED\"\n\n[mcp_servers.files]\ncommand = \"npx\"\n"
    });
    let restored = restore_own_settings(&backup, &current, &owned, TOML_WITH_LOGIN, &markers());

    // The login follows the backup, as the inherited restore does.
    assert_eq!(restored["auth"], backup["auth"]);
    let text = restored["config"].as_str().expect("toml text");
    assert!(text.contains("# my servers"), "{text}");
    assert!(text.contains("[mcp_servers.files]"), "{text}");
    assert!(
        text.contains("base_url = \"https://relay.example.com/v1\""),
        "{text}"
    );
    assert!(!text.contains("PROXY_MANAGED"), "{text}");
    assert!(!text.contains("127.0.0.1"), "{text}");
}

#[test]
fn a_toml_table_the_route_added_is_removed_whole() {
    let backup = json!({ "config": "model = \"a\"\n" });
    let routed = json!({
        "config": "model = \"a\"\nmodel_provider = \"aimanager\"\n\n[model_providers.aimanager]\nbase_url = \"http://127.0.0.1:15721/v1\"\nwire_api = \"responses\"\n"
    });
    let layout = &[LivePart::Toml("config")];
    let owned = owned_settings(&backup, &routed, layout);
    let restored = restore_own_settings(&backup, &routed, &owned, layout, &markers());
    let text = restored["config"].as_str().expect("toml text");
    assert_eq!(text.trim(), "model = \"a\"");
}

#[test]
fn nothing_owned_and_nothing_pointing_at_the_route_leaves_the_current_file() {
    let backup = json!({ "env": { "A": "1" } });
    let current = json!({ "env": { "A": "2", "B": "3" } });
    let restored = restore_own_settings(&backup, &current, &[], DOCUMENT, &markers());
    assert_eq!(restored, current);
}

#[test]
fn an_effort_chosen_while_routed_survives_the_end_of_the_route() {
    let backup = json!({
        "env": { "ANTHROPIC_BASE_URL": "https://relay.example.com" },
        "effortLevel": "low",
        "modelSettings": { "claude-opus-5-5": { "effortLevel": "low" } }
    });
    let routed = json!({
        "env": { "ANTHROPIC_BASE_URL": "http://127.0.0.1:15721" },
        "effortLevel": "low",
        "modelSettings": { "claude-opus-5-5": { "effortLevel": "low" } }
    });
    let owned = owned_settings(&backup, &routed, DOCUMENT);

    // While routed, every model was set to xhigh, then max was chosen.
    let mut current = routed.clone();
    current["effortLevel"] = json!("xhigh");
    current["modelSettings"] = json!({
        "claude-opus-5-5": { "effortLevel": "xhigh" },
        "claude-sonnet-5": { "effortLevel": "xhigh" }
    });
    current["env"]["CLAUDE_CODE_EFFORT_LEVEL"] = json!("max");

    let restored = restore_own_settings(&backup, &current, &owned, DOCUMENT, &markers());
    assert_eq!(
        restored,
        json!({
            "env": {
                "ANTHROPIC_BASE_URL": "https://relay.example.com",
                "CLAUDE_CODE_EFFORT_LEVEL": "max"
            },
            "effortLevel": "xhigh",
            "modelSettings": {
                "claude-opus-5-5": { "effortLevel": "xhigh" },
                "claude-sonnet-5": { "effortLevel": "xhigh" }
            }
        })
    );
}
