use std::collections::HashMap;
use std::fs;

use serde_json::json;

use ai_manager_lib::{
    get_claude_mcp_path, get_claude_settings_path, get_grok_config_path, read_claude_mcp_json,
    update_settings, AppError, AppSettings, AppType, McpApps, McpServer, McpService,
    MultiAppConfig, ProviderService,
};

#[path = "support.rs"]
mod support;
use support::{
    create_test_state, create_test_state_with_config, ensure_test_home, product_data_dir,
    reset_test_fs, test_mutex,
};

#[test]
fn import_default_config_claude_persists_provider() {
    let _guard = test_mutex().lock().expect("acquire test mutex");
    reset_test_fs();

    let settings_path = get_claude_settings_path();
    if let Some(parent) = settings_path.parent() {
        fs::create_dir_all(parent).expect("create claude settings dir");
    }
    let settings = json!({
        "env": {
            "ANTHROPIC_AUTH_TOKEN": "test-key",
            "ANTHROPIC_BASE_URL": "https://api.test"
        }
    });
    fs::write(
        &settings_path,
        serde_json::to_string_pretty(&settings).expect("serialize settings"),
    )
    .expect("seed claude settings.json");

    let mut config = MultiAppConfig::default();
    config.ensure_app(&AppType::Claude);
    let state = create_test_state_with_config(&config).expect("create test state");

    ProviderService::import_default_config(&state, AppType::Claude)
        .expect("import default config succeeds");

    // Verify the in-memory state.
    let providers = state
        .db
        .get_all_providers(AppType::Claude.as_str())
        .expect("get all providers");
    let current_id = state
        .db
        .get_current_provider(AppType::Claude.as_str())
        .expect("get current provider");
    assert_eq!(current_id.as_deref(), Some("default"));
    let default_provider = providers.get("default").expect("default provider");
    assert_eq!(
        default_provider.settings_config, settings,
        "default provider should capture live settings"
    );

    // Verify the data was persisted to the database (v3.7.0+ uses SQLite, not config.json).
    let db_path = product_data_dir().join("app.db");
    assert!(
        db_path.exists(),
        "importing default config should persist to app.db"
    );
}

#[test]
fn startup_import_grokbuild_official_live_does_not_resurrect_official() {
    let _guard = test_mutex().lock().expect("acquire test mutex");
    reset_test_fs();
    let _home = ensure_test_home();

    // Startup auto-import goes through the service layer (the lib.rs startup loop calls it
    // directly): official-mode live must error out and create no entry. The project-wide
    // convention is that startup auto-import only ever produces `default`, never an official
    // entry, otherwise a deleted official entry would come back on every restart. Successful
    // official-mode import only exists in the manual-import command layer.
    let config_path = get_grok_config_path();
    if let Some(parent) = config_path.parent() {
        fs::create_dir_all(parent).expect("create grok config dir");
    }
    fs::write(&config_path, "").expect("seed empty official-mode grok config.toml");

    let mut config = MultiAppConfig::default();
    config.ensure_app(&AppType::GrokBuild);
    let state = create_test_state_with_config(&config).expect("create test state");

    ProviderService::import_default_config(&state, AppType::GrokBuild)
        .expect_err("startup auto-import must not import official-mode live");

    let providers = state
        .db
        .get_all_providers(AppType::GrokBuild.as_str())
        .expect("get all providers");
    assert!(
        providers.is_empty(),
        "startup auto-import must not create any provider from official-mode live"
    );
}

#[test]
fn import_default_config_without_live_file_returns_error() {
    use support::create_test_state;

    let _guard = test_mutex().lock().expect("acquire test mutex");
    reset_test_fs();
    let _home = ensure_test_home();

    let state = create_test_state().expect("create test state");

    let err = ProviderService::import_default_config(&state, AppType::Claude)
        .expect_err("missing live file should error");
    match err {
        AppError::Localized { key, .. } => {
            assert_eq!(key, "claude.live.missing", "unexpected error key: {key}")
        }
        AppError::Message(msg) => assert!(
            msg.contains("Claude settings file is missing"),
            "unexpected error message: {msg}"
        ),
        other => panic!("unexpected error variant: {other:?}"),
    }

    // The database-backed architecture no longer inspects config.json.
    // A failed import must not write any provider to the database.
    let providers = state
        .db
        .get_all_providers(AppType::Claude.as_str())
        .expect("get all providers");
    assert!(
        providers.is_empty(),
        "failed import should not create any providers in database"
    );
}

#[test]
fn import_mcp_from_claude_creates_config_and_enables_servers() {
    let _guard = test_mutex().lock().expect("acquire test mutex");
    reset_test_fs();

    let mcp_path = get_claude_mcp_path();
    let claude_json = json!({
        "mcpServers": {
            "echo": {
                "type": "stdio",
                "command": "echo"
            }
        }
    });
    fs::write(
        &mcp_path,
        serde_json::to_string_pretty(&claude_json).expect("serialize claude mcp"),
    )
    .expect("seed ~/.claude.json");

    let config = MultiAppConfig::default();
    let state = create_test_state_with_config(&config).expect("create test state");

    let changed = McpService::import_from_claude(&state).expect("import mcp from claude succeeds");
    assert!(
        changed > 0,
        "import should report inserted or normalized entries"
    );

    let servers = state.db.get_all_mcp_servers().expect("get all mcp servers");
    let entry = servers
        .get("echo")
        .expect("server imported into unified structure");
    assert!(
        entry.apps.claude,
        "imported server should have Claude app enabled"
    );

    // Verify the data was persisted to the database.
    let db_path = product_data_dir().join("app.db");
    assert!(
        db_path.exists(),
        "state.save should persist to app.db when changes detected"
    );
}

#[test]
fn import_mcp_from_codex_does_not_rewrite_codex_config() {
    let _guard = test_mutex().lock().expect("acquire test mutex");
    reset_test_fs();
    let home = ensure_test_home();

    let codex_dir = home.join(".codex");
    fs::create_dir_all(&codex_dir).expect("create codex dir");
    let config_path = codex_dir.join("config.toml");
    let original = r#"# keep user formatting intact
model = "gpt-5"

[mcp.servers.legacy]
type = "stdio"
command = "echo"

[mcp_servers.echo]
type = "stdio"
command = "echo"
"#;
    fs::write(&config_path, original).expect("seed codex config");

    let state = create_test_state().expect("create test state");
    let changed = McpService::import_from_codex(&state).expect("import from codex");
    assert!(changed > 0, "should import servers from Codex config");

    let after = fs::read_to_string(&config_path).expect("read codex config");
    assert_eq!(
        after, original,
        "importing from Codex should not rewrite ~/.codex/config.toml"
    );
}

#[test]
fn import_mcp_from_codex_infers_official_url_only_servers_as_http() {
    let _guard = test_mutex().lock().expect("acquire test mutex");
    reset_test_fs();
    let home = ensure_test_home();

    let codex_dir = home.join(".codex");
    fs::create_dir_all(&codex_dir).expect("create codex dir");
    let config_path = codex_dir.join("config.toml");
    let original = r#"[mcp_servers.official_docs]
url = "https://developers.openai.com/mcp"
"#;
    fs::write(&config_path, original).expect("seed official remote MCP syntax");

    let state = create_test_state().expect("create test state");
    let changed = McpService::import_from_codex(&state).expect("import from codex");
    assert!(changed > 0, "the URL-only server must not be skipped");

    let servers = state.db.get_all_mcp_servers().expect("get all MCP servers");
    let entry = servers
        .get("official_docs")
        .expect("official remote MCP was imported");
    assert!(entry.apps.codex);
    assert_eq!(entry.server.get("type"), Some(&json!("http")));
    assert_eq!(
        entry.server.get("url"),
        Some(&json!("https://developers.openai.com/mcp"))
    );
    assert_eq!(
        fs::read_to_string(&config_path).expect("read source config"),
        original,
        "read-only discovery must preserve the exact Codex config bytes"
    );
}

#[test]
fn import_mcp_from_claude_does_not_sync_existing_codex_enabled_server() {
    let _guard = test_mutex().lock().expect("acquire test mutex");
    reset_test_fs();
    let home = ensure_test_home();

    let codex_dir = home.join(".codex");
    fs::create_dir_all(&codex_dir).expect("create codex dir");
    let codex_config_path = codex_dir.join("config.toml");
    let codex_original = r#"[mcp.servers.keep_me]
type = "stdio"
command = "echo"
"#;
    fs::write(&codex_config_path, codex_original).expect("seed codex config");

    let claude_json = json!({
        "mcpServers": {
            "shared": {
                "type": "stdio",
                "command": "echo"
            }
        }
    });
    fs::write(
        get_claude_mcp_path(),
        serde_json::to_string_pretty(&claude_json).expect("serialize claude mcp"),
    )
    .expect("seed claude mcp");

    let state = create_test_state().expect("create test state");
    state
        .db
        .save_mcp_server(&McpServer {
            id: "shared".to_string(),
            name: "shared".to_string(),
            server: json!({
                "type": "stdio",
                "command": "echo"
            }),
            apps: McpApps {
                claude: false,
                claude_desktop: false,
                codex: true,
                gemini: false,
                grokbuild: false,
                opencode: false,
                hermes: false,
            },
            description: None,
            homepage: None,
            docs: None,
            tags: Vec::new(),
        })
        .expect("seed existing mcp server");

    let changed = McpService::import_from_claude(&state).expect("import from claude");
    assert_eq!(changed, 0, "existing server should not count as new");

    let after = fs::read_to_string(&codex_config_path).expect("read codex config");
    assert_eq!(
        after, codex_original,
        "importing from Claude should not sync an existing Codex-enabled server"
    );

    let servers = state.db.get_all_mcp_servers().expect("get all mcp servers");
    let shared = servers.get("shared").expect("shared server exists");
    assert!(
        shared.apps.claude,
        "import should enable Claude in database"
    );
    assert!(shared.apps.codex, "existing Codex flag should be preserved");
}

#[test]
fn import_mcp_from_claude_invalid_json_preserves_state() {
    use support::create_test_state;

    let _guard = test_mutex().lock().expect("acquire test mutex");
    reset_test_fs();
    let _home = ensure_test_home();

    let mcp_path = get_claude_mcp_path();
    fs::write(&mcp_path, "{\"mcpServers\":") // truncated JSON
        .expect("seed invalid ~/.claude.json");

    let state = create_test_state().expect("create test state");

    let err =
        McpService::import_from_claude(&state).expect_err("invalid json should bubble up error");
    match err {
        AppError::McpValidation(msg) => assert!(
            msg.contains("Failed to parse ~/.claude.json"),
            "unexpected error message: {msg}"
        ),
        other => panic!("unexpected error variant: {other:?}"),
    }

    // Database-backed architecture: check that no MCP server was written.
    let servers = state.db.get_all_mcp_servers().expect("get all mcp servers");
    assert!(
        servers.is_empty(),
        "failed import should not persist any MCP servers to database"
    );
}

/// "Import from apps" is best-effort: one app's broken config file must not block the
/// other apps' imports, but the failures must be reported in aggregate. The historical
/// implementation swallowed errors with a per-app `unwrap_or(0)`, so a broken config.toml
/// only showed up as "imported 0" and the user had no way to learn what went wrong.
#[test]
fn import_from_all_apps_reports_broken_app_but_imports_the_rest() {
    use support::create_test_state;

    let _guard = test_mutex().lock().expect("acquire test mutex");
    reset_test_fs();
    let home = ensure_test_home();

    // A healthy ~/.claude.json: should import normally.
    let claude_json = json!({
        "mcpServers": {
            "alpha": { "type": "stdio", "command": "echo" }
        }
    });
    fs::write(
        get_claude_mcp_path(),
        serde_json::to_string_pretty(&claude_json).expect("serialize claude mcp"),
    )
    .expect("seed ~/.claude.json");

    // A broken ~/.codex/config.toml: parsing is guaranteed to fail.
    let codex_dir = home.join(".codex");
    fs::create_dir_all(&codex_dir).expect("create codex dir");
    fs::write(codex_dir.join("config.toml"), "not = = valid toml")
        .expect("seed broken codex config");

    let state = create_test_state().expect("create test state");

    let err = McpService::import_from_all_apps(&state)
        .expect_err("broken codex config must surface, not be swallowed as zero imports");
    let message = err.to_string();
    assert!(
        message.contains("codex"),
        "aggregated error should name the failing app, got: {message}"
    );

    // The Codex failure must not block Claude: alpha should be stored with Claude enabled.
    let servers = state.db.get_all_mcp_servers().expect("get all mcp servers");
    let entry = servers
        .get("alpha")
        .expect("claude server imported despite codex failure");
    assert!(
        entry.apps.claude,
        "imported server should have Claude app enabled"
    );
}

#[test]
fn set_mcp_enabled_for_codex_writes_live_config() {
    let _guard = test_mutex().lock().expect("acquire test mutex");
    reset_test_fs();
    let home = ensure_test_home();

    // Create the Codex config directory and files.
    let codex_dir = home.join(".codex");
    fs::create_dir_all(&codex_dir).expect("create codex dir");
    fs::write(
        codex_dir.join("auth.json"),
        r#"{"OPENAI_API_KEY":"test-key"}"#,
    )
    .expect("create auth.json");
    fs::write(codex_dir.join("config.toml"), "").expect("create empty config.toml");

    let mut config = MultiAppConfig::default();
    config.ensure_app(&AppType::Codex);

    // v3.7.0: use the unified structure.
    config.mcp.servers = Some(HashMap::new());
    config.mcp.servers.as_mut().unwrap().insert(
        "codex-server".into(),
        McpServer {
            id: "codex-server".to_string(),
            name: "Codex Server".to_string(),
            server: json!({
                "type": "stdio",
                "command": "echo"
            }),
            apps: McpApps {
                claude: false,
                claude_desktop: false,
                codex: false, // disabled initially
                gemini: false,
                grokbuild: false,
                opencode: false,
                hermes: false,
            },
            description: None,
            homepage: None,
            docs: None,
            tags: Vec::new(),
        },
    );

    let state = create_test_state_with_config(&config).expect("create test state");

    // v3.7.0: toggle_app replaces set_enabled.
    McpService::toggle_app(&state, "codex-server", AppType::Codex, true)
        .expect("toggle_app should succeed");

    let servers = state.db.get_all_mcp_servers().expect("get all mcp servers");
    let entry = servers.get("codex-server").expect("codex server exists");
    assert!(
        entry.apps.codex,
        "server should have Codex app enabled after toggle"
    );

    let toml_path = ai_manager_lib::get_codex_config_path();
    assert!(
        toml_path.exists(),
        "enabling server should trigger sync to ~/.codex/config.toml"
    );
    let toml_text = fs::read_to_string(&toml_path).expect("read codex config");
    assert!(
        toml_text.contains("codex-server"),
        "codex config should include the enabled server definition"
    );
}

#[test]
fn enabling_codex_mcp_skips_when_codex_dir_missing() {
    use support::create_test_state;

    let _guard = test_mutex().lock().expect("acquire test mutex");
    reset_test_fs();
    let home = ensure_test_home();

    // Confirm the Codex config dir is absent (simulating "Codex CLI never installed/run").
    assert!(
        !home.join(".codex").exists(),
        "~/.codex should not exist in fresh test environment"
    );

    let state = create_test_state().expect("create test state");

    // Insert an MCP server with Codex disabled first, so upsert does not trigger a sync.
    McpService::upsert_server(
        &state,
        McpServer {
            id: "codex-server".to_string(),
            name: "Codex Server".to_string(),
            server: json!({
                "type": "stdio",
                "command": "echo"
            }),
            apps: McpApps {
                claude: false,
                claude_desktop: false,
                codex: false,
                gemini: false,
                grokbuild: false,
                opencode: false,
                hermes: false,
            },
            description: None,
            homepage: None,
            docs: None,
            tags: Vec::new(),
        },
    )
    .expect("insert server without syncing");

    // Enable Codex: with the directory missing the write must be skipped (no ~/.codex/config.toml).
    McpService::toggle_app(&state, "codex-server", AppType::Codex, true)
        .expect("toggle codex should succeed even when ~/.codex is missing");

    assert!(
        !home.join(".codex").exists(),
        "~/.codex should still not exist after skipped sync"
    );
}

#[test]
fn upsert_mcp_server_disabling_app_removes_from_claude_live_config() {
    let _guard = test_mutex().lock().expect("acquire test mutex");
    reset_test_fs();
    let home = ensure_test_home();

    // Simulate Claude being installed/initialized: ~/.claude exists.
    fs::create_dir_all(home.join(".claude")).expect("create ~/.claude dir");

    // Create an MCP server with Claude enabled first.
    let state = support::create_test_state().expect("create test state");
    McpService::upsert_server(
        &state,
        McpServer {
            id: "echo".to_string(),
            name: "echo".to_string(),
            server: json!({
                "type": "stdio",
                "command": "echo"
            }),
            apps: McpApps {
                claude: true,
                claude_desktop: false,
                codex: false,
                gemini: false,
                grokbuild: false,
                opencode: false,
                hermes: false,
            },
            description: None,
            homepage: None,
            docs: None,
            tags: Vec::new(),
        },
    )
    .expect("upsert should sync to Claude live config");

    // Confirm it was written to ~/.claude.json.
    let mcp_path = get_claude_mcp_path();
    let text = fs::read_to_string(&mcp_path).expect("read ~/.claude.json");
    let v: serde_json::Value = serde_json::from_str(&text).expect("parse ~/.claude.json");
    assert!(
        v.pointer("/mcpServers/echo").is_some(),
        "echo should exist in Claude live config after enabling"
    );

    // Upsert again with Claude unchecked (apps.claude=false): it must be removed from the Claude live config.
    McpService::upsert_server(
        &state,
        McpServer {
            id: "echo".to_string(),
            name: "echo".to_string(),
            server: json!({
                "type": "stdio",
                "command": "echo"
            }),
            apps: McpApps {
                claude: false,
                claude_desktop: false,
                codex: false,
                gemini: false,
                grokbuild: false,
                opencode: false,
                hermes: false,
            },
            description: None,
            homepage: None,
            docs: None,
            tags: Vec::new(),
        },
    )
    .expect("upsert disabling app should remove from Claude live config");

    let text = fs::read_to_string(&mcp_path).expect("read ~/.claude.json after disable");
    let v: serde_json::Value = serde_json::from_str(&text).expect("parse ~/.claude.json");
    assert!(
        v.pointer("/mcpServers/echo").is_none(),
        "echo should be removed from Claude live config after disabling"
    );
}

#[test]
fn import_mcp_from_multiple_apps_merges_enabled_flags() {
    let _guard = test_mutex().lock().expect("acquire test mutex");
    reset_test_fs();
    let home = ensure_test_home();

    // 1) Claude: ~/.claude.json
    let mcp_path = get_claude_mcp_path();
    let claude_json = json!({
        "mcpServers": {
            "shared": {
                "type": "stdio",
                "command": "echo"
            }
        }
    });
    fs::write(
        &mcp_path,
        serde_json::to_string_pretty(&claude_json).expect("serialize claude mcp"),
    )
    .expect("seed ~/.claude.json");

    // 2) Codex: ~/.codex/config.toml
    let codex_dir = home.join(".codex");
    fs::create_dir_all(&codex_dir).expect("create codex dir");
    fs::write(
        codex_dir.join("config.toml"),
        r#"[mcp_servers.shared]
type = "stdio"
command = "echo"
"#,
    )
    .expect("seed ~/.codex/config.toml");

    let state = support::create_test_state().expect("create test state");

    McpService::import_from_claude(&state).expect("import from claude");
    McpService::import_from_codex(&state).expect("import from codex");

    let servers = state.db.get_all_mcp_servers().expect("get all mcp servers");
    let entry = servers.get("shared").expect("shared server exists");
    assert!(entry.apps.claude, "shared should enable Claude");
    assert!(entry.apps.codex, "shared should enable Codex");
}

#[test]
fn import_mcp_from_gemini_sse_url_only_is_valid() {
    let _guard = test_mutex().lock().expect("acquire test mutex");
    reset_test_fs();
    let home = ensure_test_home();

    // Gemini MCP lives in ~/.gemini/settings.json.
    let gemini_dir = home.join(".gemini");
    fs::create_dir_all(&gemini_dir).expect("create gemini dir");
    let settings_path = gemini_dir.join("settings.json");

    // Gemini SSE: url only (Gemini does not use the type field).
    let gemini_settings = json!({
        "mcpServers": {
            "sse-server": {
                "url": "https://example.com/sse"
            }
        }
    });
    fs::write(
        &settings_path,
        serde_json::to_string_pretty(&gemini_settings).expect("serialize gemini settings"),
    )
    .expect("seed ~/.gemini/settings.json");

    let state = support::create_test_state().expect("create test state");
    let changed = McpService::import_from_gemini(&state).expect("import from gemini");
    assert!(changed > 0, "should import at least 1 server");

    let servers = state.db.get_all_mcp_servers().expect("get all mcp servers");
    let entry = servers.get("sse-server").expect("sse-server exists");
    assert!(entry.apps.gemini, "imported server should enable Gemini");
    assert_eq!(
        entry.server.get("type").and_then(|v| v.as_str()),
        Some("sse"),
        "Gemini url-only server should be normalized to type=sse in unified structure"
    );
}

#[test]
fn enabling_gemini_mcp_skips_when_gemini_dir_missing() {
    use support::create_test_state;

    let _guard = test_mutex().lock().expect("acquire test mutex");
    reset_test_fs();
    let home = ensure_test_home();

    // Confirm the Gemini config dir is absent (simulating "Gemini CLI never installed/run").
    assert!(
        !home.join(".gemini").exists(),
        "~/.gemini should not exist in fresh test environment"
    );

    let state = create_test_state().expect("create test state");

    // Insert an MCP server with Gemini disabled first, so upsert does not trigger a sync.
    McpService::upsert_server(
        &state,
        McpServer {
            id: "gemini-server".to_string(),
            name: "Gemini Server".to_string(),
            server: json!({
                "type": "sse",
                "url": "https://example.com/sse"
            }),
            apps: McpApps {
                claude: false,
                claude_desktop: false,
                codex: false,
                gemini: false,
                grokbuild: false,
                opencode: false,
                hermes: false,
            },
            description: None,
            homepage: None,
            docs: None,
            tags: Vec::new(),
        },
    )
    .expect("insert server without syncing");

    // Enable Gemini: with the directory missing the write must be skipped (no ~/.gemini/settings.json).
    McpService::toggle_app(&state, "gemini-server", AppType::Gemini, true)
        .expect("toggle gemini should succeed even when ~/.gemini is missing");

    assert!(
        !home.join(".gemini").exists(),
        "~/.gemini should still not exist after skipped sync"
    );
}

#[test]
fn enabling_claude_mcp_skips_when_claude_config_absent() {
    use support::create_test_state;

    let _guard = test_mutex().lock().expect("acquire test mutex");
    reset_test_fs();
    let home = ensure_test_home();

    // Confirm no Claude directory/file exists (simulating "Claude never installed/run").
    assert!(
        !home.join(".claude").exists(),
        "~/.claude should not exist in fresh test environment"
    );
    assert!(
        !home.join(".claude.json").exists(),
        "~/.claude.json should not exist in fresh test environment"
    );

    let state = create_test_state().expect("create test state");

    // Insert an MCP server with Claude disabled first, so upsert does not trigger a sync.
    McpService::upsert_server(
        &state,
        McpServer {
            id: "claude-server".to_string(),
            name: "Claude Server".to_string(),
            server: json!({
                "type": "stdio",
                "command": "echo"
            }),
            apps: McpApps {
                claude: false,
                claude_desktop: false,
                codex: false,
                gemini: false,
                grokbuild: false,
                opencode: false,
                hermes: false,
            },
            description: None,
            homepage: None,
            docs: None,
            tags: Vec::new(),
        },
    )
    .expect("insert server without syncing");

    // Enable Claude: with the config missing the write must be skipped (no ~/.claude.json).
    McpService::toggle_app(&state, "claude-server", AppType::Claude, true)
        .expect("toggle claude should succeed even when ~/.claude is missing");

    assert!(
        !home.join(".claude.json").exists(),
        "~/.claude.json should still not exist after skipped sync"
    );
}

#[test]
fn explicit_default_claude_dir_keeps_default_split_mcp_path() {
    let _guard = test_mutex().lock().expect("acquire test mutex");
    reset_test_fs();
    let home = ensure_test_home();
    let claude_dir = home.join(".claude");
    fs::create_dir_all(&claude_dir).expect("create explicit default claude dir");

    update_settings(AppSettings {
        claude_config_dir: Some(claude_dir.to_string_lossy().to_string()),
        ..AppSettings::default()
    })
    .expect("set explicit default claude config dir");

    assert_eq!(
        get_claude_mcp_path(),
        home.join(".claude.json"),
        "explicit default Claude dir should keep Claude Code's split MCP path"
    );

    let state = create_test_state().expect("create test state");
    McpService::upsert_server(
        &state,
        McpServer {
            id: "claude-default".to_string(),
            name: "Claude Default".to_string(),
            server: json!({
                "type": "stdio",
                "command": "echo"
            }),
            apps: McpApps {
                claude: true,
                claude_desktop: false,
                codex: false,
                gemini: false,
                grokbuild: false,
                opencode: false,
                hermes: false,
            },
            description: None,
            homepage: None,
            docs: None,
            tags: Vec::new(),
        },
    )
    .expect("sync default Claude MCP");

    assert!(
        home.join(".claude.json").exists(),
        "default split MCP file should be written at home/.claude.json"
    );
    assert!(
        !claude_dir.join(".claude.json").exists(),
        "explicit default dir should not use nested .claude/.claude.json"
    );
}

#[test]
fn custom_claude_dir_writes_mcp_inside_config_dir() {
    let _guard = test_mutex().lock().expect("acquire test mutex");
    reset_test_fs();
    let home = ensure_test_home();
    let custom_dir = home.join("profiles").join(".claude");
    fs::create_dir_all(&custom_dir).expect("create custom claude dir");

    update_settings(AppSettings {
        claude_config_dir: Some(custom_dir.to_string_lossy().to_string()),
        ..AppSettings::default()
    })
    .expect("set custom claude config dir");

    let expected_mcp_path = custom_dir.join(".claude.json");
    assert_eq!(
        get_claude_mcp_path(),
        expected_mcp_path,
        "custom Claude dir should keep MCP state inside the config dir"
    );

    let state = create_test_state().expect("create test state");
    McpService::upsert_server(
        &state,
        McpServer {
            id: "claude-custom".to_string(),
            name: "Claude Custom".to_string(),
            server: json!({
                "type": "stdio",
                "command": "echo"
            }),
            apps: McpApps {
                claude: true,
                claude_desktop: false,
                codex: false,
                gemini: false,
                grokbuild: false,
                opencode: false,
                hermes: false,
            },
            description: None,
            homepage: None,
            docs: None,
            tags: Vec::new(),
        },
    )
    .expect("sync custom Claude MCP");

    assert!(
        expected_mcp_path.exists(),
        "custom Claude MCP file should be written inside custom dir"
    );
    assert!(
        !home.join("profiles").join(".claude.json").exists(),
        "custom Claude dir should not write sibling .claude.json"
    );
}

#[test]
fn custom_claude_dir_sync_does_not_copy_default_profile() {
    let _guard = test_mutex().lock().expect("acquire test mutex");
    reset_test_fs();
    let home = ensure_test_home();
    let home_mcp_path = home.join(".claude.json");
    let default_profile = json!({
        "hasCompletedOnboarding": true,
        "projects": {
            "/home-project": {
                "hasTrustDialogAccepted": true
            }
        },
        "mcpServers": {
            "home-only": {
                "type": "stdio",
                "command": "home-command"
            }
        },
        "profileSentinel": "home-profile"
    });
    let default_profile_text =
        serde_json::to_string_pretty(&default_profile).expect("serialize default profile");
    fs::write(&home_mcp_path, &default_profile_text).expect("seed default Claude profile");

    let custom_dir = home.join("profiles").join("work").join(".claude");
    fs::create_dir_all(&custom_dir).expect("create custom claude dir");
    update_settings(AppSettings {
        claude_config_dir: Some(custom_dir.to_string_lossy().to_string()),
        ..AppSettings::default()
    })
    .expect("set custom claude config dir");

    let expected_mcp_path = custom_dir.join(".claude.json");
    assert_eq!(
        get_claude_mcp_path(),
        expected_mcp_path,
        "custom Claude dir should use nested .claude.json"
    );
    assert!(
        !expected_mcp_path.exists(),
        "custom profile should start without a live MCP file"
    );

    let state = create_test_state().expect("create test state");
    McpService::upsert_server(
        &state,
        McpServer {
            id: "custom-only".to_string(),
            name: "Custom Only".to_string(),
            server: json!({
                "type": "stdio",
                "command": "custom-command"
            }),
            apps: McpApps {
                claude: true,
                claude_desktop: false,
                codex: false,
                gemini: false,
                grokbuild: false,
                opencode: false,
                hermes: false,
            },
            description: None,
            homepage: None,
            docs: None,
            tags: Vec::new(),
        },
    )
    .expect("sync custom Claude MCP");

    let text = fs::read_to_string(&expected_mcp_path).expect("read custom Claude MCP");
    let value: serde_json::Value = serde_json::from_str(&text).expect("parse custom Claude MCP");
    let servers = value
        .get("mcpServers")
        .and_then(|v| v.as_object())
        .expect("custom profile should contain mcpServers");
    assert!(
        servers.contains_key("custom-only"),
        "custom profile should contain DB-managed Claude server"
    );
    assert!(
        !servers.contains_key("home-only"),
        "custom profile should not inherit default profile MCP servers"
    );
    assert!(
        value.get("hasCompletedOnboarding").is_none(),
        "custom profile should not inherit onboarding state"
    );
    assert!(
        value.get("projects").is_none(),
        "custom profile should not inherit project trust state"
    );
    assert!(
        value.get("profileSentinel").is_none(),
        "custom profile should not inherit unrelated default profile fields"
    );
    assert_eq!(
        fs::read_to_string(&home_mcp_path).expect("reread default Claude profile"),
        default_profile_text,
        "default Claude profile should remain unchanged"
    );
}

#[test]
fn custom_claude_dir_read_only_mcp_queries_do_not_create_profile() {
    let _guard = test_mutex().lock().expect("acquire test mutex");
    reset_test_fs();
    let home = ensure_test_home();
    let home_mcp_path = home.join(".claude.json");
    fs::write(
        &home_mcp_path,
        serde_json::to_string_pretty(&json!({
            "mcpServers": {
                "home-only": {
                    "type": "stdio",
                    "command": "home-command"
                }
            },
            "profileSentinel": "home-profile"
        }))
        .expect("serialize default profile"),
    )
    .expect("seed default Claude profile");

    let custom_dir = home.join("profiles").join("work").join(".claude");
    fs::create_dir_all(&custom_dir).expect("create custom claude dir");
    update_settings(AppSettings {
        claude_config_dir: Some(custom_dir.to_string_lossy().to_string()),
        ..AppSettings::default()
    })
    .expect("set custom claude config dir");

    let expected_mcp_path = custom_dir.join(".claude.json");
    assert!(
        !expected_mcp_path.exists(),
        "custom profile should start without a live MCP file"
    );

    assert_eq!(
        get_claude_mcp_path(),
        expected_mcp_path,
        "path resolution should report the custom profile MCP path"
    );
    let text = read_claude_mcp_json().expect("read Claude MCP config");
    assert_eq!(text, None, "missing custom profile should read as None");
    assert!(
        !expected_mcp_path.exists(),
        "read-only MCP queries should not copy or create the custom profile"
    );
}

#[test]
fn sync_all_enabled_removes_known_disabled_but_preserves_unknown_live_entries() {
    let _guard = test_mutex().lock().expect("acquire test mutex");
    reset_test_fs();
    let _home = ensure_test_home();

    let mcp_path = get_claude_mcp_path();
    fs::write(
        &mcp_path,
        serde_json::to_string_pretty(&json!({
            "mcpServers": {
                "managed-disabled": {
                    "type": "stdio",
                    "command": "echo"
                },
                "external-only": {
                    "type": "stdio",
                    "command": "external"
                }
            }
        }))
        .expect("serialize claude mcp"),
    )
    .expect("seed claude mcp");

    let state = create_test_state().expect("create test state");

    state
        .db
        .save_mcp_server(&McpServer {
            id: "managed-disabled".to_string(),
            name: "Managed Disabled".to_string(),
            server: json!({
                "type": "stdio",
                "command": "echo"
            }),
            apps: McpApps {
                claude: false,
                claude_desktop: false,
                codex: false,
                gemini: false,
                grokbuild: false,
                opencode: false,
                hermes: false,
            },
            description: None,
            homepage: None,
            docs: None,
            tags: Vec::new(),
        })
        .expect("save disabled server");
    state
        .db
        .save_mcp_server(&McpServer {
            id: "managed-enabled".to_string(),
            name: "Managed Enabled".to_string(),
            server: json!({
                "type": "stdio",
                "command": "managed"
            }),
            apps: McpApps {
                claude: true,
                claude_desktop: false,
                codex: false,
                gemini: false,
                grokbuild: false,
                opencode: false,
                hermes: false,
            },
            description: None,
            homepage: None,
            docs: None,
            tags: Vec::new(),
        })
        .expect("save enabled server");

    McpService::sync_all_enabled(&state).expect("reconcile mcp");

    let text = fs::read_to_string(&mcp_path).expect("read claude mcp");
    let value: serde_json::Value = serde_json::from_str(&text).expect("parse claude mcp");
    let servers = value
        .get("mcpServers")
        .and_then(|entry| entry.as_object())
        .expect("mcpServers object");

    assert!(
        !servers.contains_key("managed-disabled"),
        "DB-known disabled server should be removed from live config"
    );
    assert!(
        servers.contains_key("managed-enabled"),
        "DB-known enabled server should be present in live config"
    );
    assert!(
        servers.contains_key("external-only"),
        "live entries unknown to DB should be preserved"
    );
}
