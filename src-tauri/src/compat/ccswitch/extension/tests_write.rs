use super::prompt::guard_enabled;
use super::{not_found, reject_disable, toggle_failed};
use crate::domain::{
    DesktopAppId, ErrorCode, ExtensionKind, ExtensionManagement, ExtensionScope,
    McpConnectionDraft, McpInstallDraft, ToolId,
};

fn scope(tool: ToolId) -> ExtensionScope {
    ExtensionScope::tool(tool)
}

#[test]
fn a_missing_extension_is_reported_as_a_translatable_not_found() {
    // The upstream McpService::toggle_app silently returns Ok(()) for a non-existent id
    // (services/mcp.rs:78), so "does it still exist" has to be answered by us.
    let error = not_found(scope(ToolId::ClaudeCode), ExtensionKind::Mcp, "filesystem");
    assert_eq!(error.code, ErrorCode::ExtensionNotFound);
    assert_eq!(error.message_key, "error.extension.notFound");
    assert_eq!(
        error.technical_message.as_deref(),
        Some("tool:claude-code/mcp/filesystem")
    );
}

#[test]
fn refusing_to_switch_off_a_prompt_says_so_in_a_translatable_way() {
    let error = reject_disable();
    assert_eq!(error.code, ErrorCode::ConfigWriteFailed);
    assert_eq!(error.message_key, "error.extension.cannotDisable");
    assert!(error.remediation.is_some());
}

#[test]
fn the_prompt_guard_allows_switching_over_and_blocks_switching_off() {
    assert!(guard_enabled(true).is_ok());
    let error = guard_enabled(false).expect_err("prompts cannot be switched off");
    assert_eq!(error.message_key, "error.extension.cannotDisable");
}

#[test]
fn an_upstream_failure_keeps_its_detail_out_of_the_user_facing_key() {
    // The two upstream sides have different error types (anyhow vs the upstream AppError), so the mapper funnels them through Display.
    let from_anyhow = toggle_failed(anyhow::anyhow!(
        "Permission denied: /Users/somebody/.claude/skills"
    ));
    assert_eq!(from_anyhow.code, ErrorCode::ConfigWriteFailed);
    assert_eq!(from_anyhow.message_key, "error.extension.toggleFailed");
    let technical = from_anyhow
        .technical_message
        .clone()
        .expect("upstream detail is kept for View Details");
    assert!(technical.contains("Permission denied"));
    // A raw upstream message only ever goes into technical_message, never into the one the user sees.
    assert!(!from_anyhow.message_key.contains("Permission"));
}

#[test]
fn the_upstream_detail_is_redacted_and_truncated_before_it_is_kept() {
    let noisy = "x".repeat(5000);
    let error = toggle_failed(noisy);
    let technical = error.technical_message.expect("detail is kept");
    assert!(
        technical.len() <= 2100,
        "an unbounded upstream string reached technical_message ({} bytes)",
        technical.len()
    );
}

#[test]
fn guided_mcp_connection_specs_are_synthesized_without_secret_slots() {
    let local = super::mcp::connection_spec(&McpConnectionDraft::Stdio {
        command: " npx ".to_string(),
        arguments: vec!["-y".to_string(), "server-package".to_string()],
    });
    assert_eq!(
        local,
        serde_json::json!({
            "type": "stdio",
            "command": "npx",
            "args": ["-y", "server-package"]
        })
    );

    let remote = super::mcp::connection_spec(&McpConnectionDraft::Http {
        url: " https://mcp.example.test/v1 ".to_string(),
    });
    assert_eq!(
        remote,
        serde_json::json!({"type": "http", "url": "https://mcp.example.test/v1"})
    );
    for forbidden in ["env", "headers", "token", "authorization"] {
        assert!(!local.to_string().to_ascii_lowercase().contains(forbidden));
        assert!(!remote.to_string().to_ascii_lowercase().contains(forbidden));
    }
}

#[test]
fn product_mcp_ids_are_a_small_fixed_alphabet() {
    for valid in ["files-a1b2c3d4", "mcp-00000000"] {
        assert!(super::mcp::valid_product_id(valid), "{valid}");
    }
    for invalid in ["", "Files-a1b2c3d4", "../files", "files_name", "файл"] {
        assert!(!super::mcp::valid_product_id(invalid), "{invalid}");
    }
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
fn skill_inventory_merges_local_directories_only_into_their_real_tool_scope() {
    use std::sync::Arc;

    let temp = tempfile::tempdir().expect("temp home");
    let _home = TestHome::set(temp.path());
    for (tool_dir, skill_dir, name) in [
        (".claude", "claude-only", "Claude local"),
        (".codex", "codex-only", "Codex local"),
    ] {
        let directory = temp.path().join(tool_dir).join("skills").join(skill_dir);
        std::fs::create_dir_all(&directory).expect("create local Skill directory");
        std::fs::write(
            directory.join("SKILL.md"),
            format!("---\nname: {name}\ndescription: Local test Skill\n---\n"),
        )
        .expect("write local Skill");
    }
    let state = crate::store::AppState::new(Arc::new(
        crate::database::Database::memory().expect("memory database"),
    ));

    let claude = super::skill::list(
        &state,
        ToolId::ClaudeCode,
        &crate::app_config::AppType::Claude,
    )
    .expect("list Claude Skills");
    assert_eq!(claude.len(), 1);
    assert_eq!(claude[0].id, "claude-only");
    assert_eq!(claude[0].management, ExtensionManagement::Detected);

    let codex = super::skill::list(&state, ToolId::Codex, &crate::app_config::AppType::Codex)
        .expect("list Codex Skills");
    assert_eq!(codex.len(), 1);
    assert_eq!(codex[0].id, "codex-only");
    assert_eq!(codex[0].management, ExtensionManagement::Detected);
}

#[test]
#[serial_test::serial]
fn mcp_inventory_detects_live_only_connections_without_exposing_their_specs() {
    use std::sync::Arc;

    let temp = tempfile::tempdir().expect("temp home");
    let _home = TestHome::set(temp.path());
    std::fs::create_dir_all(temp.path().join(".claude")).expect("create Claude directory");
    std::fs::write(
        temp.path().join(".claude.json"),
        r#"{"mcpServers":{"browser":{"command":"npx","args":["secret-package"]}}}"#,
    )
    .expect("write Claude MCP config");
    std::fs::create_dir_all(temp.path().join(".codex")).expect("create Codex directory");
    std::fs::write(
        temp.path().join(".codex/config.toml"),
        "[mcp_servers.node_repl]\ncommand = \"node\"\nargs = [\"--do-not-expose\"]\n",
    )
    .expect("write Codex MCP config");
    let state = crate::store::AppState::new(Arc::new(
        crate::database::Database::memory().expect("memory database"),
    ));

    let claude = super::mcp::list(
        &state,
        scope(ToolId::ClaudeCode),
        &crate::app_config::AppType::Claude,
    )
    .expect("list Claude MCP");
    assert_eq!(claude.len(), 1);
    assert_eq!(claude[0].id, "browser");
    assert_eq!(claude[0].management, ExtensionManagement::Detected);
    let claude_wire = serde_json::to_string(&claude).expect("serialize Claude inventory");
    assert!(!claude_wire.contains("secret-package"));
    assert!(!claude_wire.contains("command"));

    let codex = super::mcp::list(
        &state,
        scope(ToolId::Codex),
        &crate::app_config::AppType::Codex,
    )
    .expect("list Codex MCP");
    assert_eq!(codex.len(), 1);
    assert_eq!(codex[0].id, "node_repl");
    assert_eq!(codex[0].management, ExtensionManagement::Detected);
    let codex_wire = serde_json::to_string(&codex).expect("serialize Codex inventory");
    assert!(!codex_wire.contains("do-not-expose"));
    assert!(!codex_wire.contains("args"));
}

#[test]
#[serial_test::serial]
fn adopting_detected_skills_records_every_live_scope_without_rewriting_source_files() {
    use std::sync::Arc;

    let temp = tempfile::tempdir().expect("temp home");
    let _home = TestHome::set(temp.path());
    let skill_body = b"---\nname: Shared local\ndescription: Already in both tools\n---\n";
    let mut sources = Vec::new();
    for tool_dir in [".claude", ".codex"] {
        let directory = temp
            .path()
            .join(tool_dir)
            .join("skills")
            .join("shared-local");
        std::fs::create_dir_all(&directory).expect("create local Skill directory");
        let source = directory.join("SKILL.md");
        std::fs::write(&source, skill_body).expect("write local Skill");
        sources.push(source);
    }
    let state = crate::store::AppState::new(Arc::new(
        crate::database::Database::memory().expect("memory database"),
    ));
    let store = super::ExtensionStore { state };

    let refreshed = store
        .adopt_detected(ToolId::ClaudeCode, ExtensionKind::Skill)
        .expect("adopt detected Skills");
    assert_eq!(refreshed.len(), 1);
    assert_eq!(refreshed[0].management, ExtensionManagement::Managed);
    assert!(refreshed[0].enabled);

    let installed = store
        .state
        .db
        .get_all_installed_skills()
        .expect("read managed Skills");
    let managed = installed.values().next().expect("one managed Skill");
    assert_eq!(managed.directory, "shared-local");
    assert!(managed.apps.claude);
    assert!(managed.apps.codex);
    for source in sources {
        assert_eq!(
            std::fs::read(source).expect("read untouched source"),
            skill_body
        );
    }

    let codex = store
        .list(ToolId::Codex, ExtensionKind::Skill)
        .expect("list Codex Skills after adoption");
    assert_eq!(codex.len(), 1);
    assert_eq!(codex[0].management, ExtensionManagement::Managed);
    assert!(codex[0].enabled);
}

#[test]
#[serial_test::serial]
fn a_shared_agents_skill_is_listed_even_before_it_has_a_tool_specific_copy() {
    use std::sync::Arc;

    let temp = tempfile::tempdir().expect("temp home");
    let _home = TestHome::set(temp.path());
    let directory = temp
        .path()
        .join(".agents")
        .join("skills")
        .join("shared-agent-skill");
    std::fs::create_dir_all(&directory).expect("create shared Skill directory");
    std::fs::write(
        directory.join("SKILL.md"),
        b"---\nname: Shared agent Skill\ndescription: Available from the shared root\n---\n",
    )
    .expect("write shared Skill");
    let state = crate::store::AppState::new(Arc::new(
        crate::database::Database::memory().expect("memory database"),
    ));
    let store = super::ExtensionStore { state };

    let claude = store
        .list(ToolId::ClaudeCode, ExtensionKind::Skill)
        .expect("list Claude Skills");
    assert_eq!(claude.len(), 1);
    assert_eq!(claude[0].id, "shared-agent-skill");
    assert_eq!(claude[0].management, ExtensionManagement::Detected);
    let wire = serde_json::to_string(&claude).expect("serialize shared inventory");
    assert!(!wire.contains(".agents"));
    assert!(!wire.contains("SKILL.md"));
}

#[test]
#[serial_test::serial]
fn adopting_a_shared_only_skill_materializes_it_in_the_chosen_tool() {
    use std::sync::Arc;

    let temp = tempfile::tempdir().expect("temp home");
    let _home = TestHome::set(temp.path());
    let shared = temp
        .path()
        .join(".agents")
        .join("skills")
        .join("shared-agent-skill");
    std::fs::create_dir_all(&shared).expect("create shared Skill directory");
    let body = b"---\nname: Shared agent Skill\ndescription: Only in the shared root\n---\n";
    std::fs::write(shared.join("SKILL.md"), body).expect("write shared Skill");
    let state = crate::store::AppState::new(Arc::new(
        crate::database::Database::memory().expect("memory database"),
    ));
    let store = super::ExtensionStore { state };

    let refreshed = store
        .adopt_detected(ToolId::ClaudeCode, ExtensionKind::Skill)
        .expect("adopt the shared Skill for Claude Code");
    assert_eq!(refreshed.len(), 1);
    assert_eq!(refreshed[0].management, ExtensionManagement::Managed);
    assert!(refreshed[0].enabled);

    // "Enabled for Claude Code" must be true on disk, not only in the database.
    let tool_copy = temp
        .path()
        .join(".claude")
        .join("skills")
        .join("shared-agent-skill")
        .join("SKILL.md");
    assert_eq!(
        std::fs::read(&tool_copy).expect("the tool now has its own copy"),
        body
    );
    assert_eq!(
        std::fs::read(shared.join("SKILL.md")).expect("shared source untouched"),
        body
    );
    let managed = store
        .state
        .db
        .get_all_installed_skills()
        .expect("read managed Skills");
    assert!(
        managed
            .values()
            .next()
            .expect("one managed Skill")
            .apps
            .claude
    );
}

#[test]
#[serial_test::serial]
fn a_skill_that_cannot_be_placed_in_the_tool_is_not_reported_as_enabled_there() {
    use std::sync::Arc;

    let temp = tempfile::tempdir().expect("temp home");
    let _home = TestHome::set(temp.path());
    let shared = temp
        .path()
        .join(".agents")
        .join("skills")
        .join("shared-agent-skill");
    std::fs::create_dir_all(&shared).expect("create shared Skill directory");
    std::fs::write(
        shared.join("SKILL.md"),
        b"---\nname: Shared agent Skill\n---\n",
    )
    .expect("write shared Skill");
    // The tool's skills root is a regular file, so nothing can be placed in it.
    std::fs::create_dir_all(temp.path().join(".claude")).expect("create Claude directory");
    std::fs::write(
        temp.path().join(".claude").join("skills"),
        b"not a directory",
    )
    .expect("block the Claude skills root");
    let state = crate::store::AppState::new(Arc::new(
        crate::database::Database::memory().expect("memory database"),
    ));
    let store = super::ExtensionStore { state };

    let error = store
        .adopt_detected(ToolId::ClaudeCode, ExtensionKind::Skill)
        .expect_err("a Skill that is not on disk for the tool cannot be reported as enabled");
    assert_eq!(error.message_key, "error.extension.adoptFailed");

    let managed = store
        .state
        .db
        .get_all_installed_skills()
        .expect("read managed Skills");
    let row = managed.values().next().expect("the Skill stays managed");
    assert!(
        !row.apps.claude,
        "the Claude flag claims a copy that does not exist"
    );
    let claude = store
        .list(ToolId::ClaudeCode, ExtensionKind::Skill)
        .expect("list Claude Skills after the failed placement");
    assert_eq!(claude.len(), 1);
    assert_eq!(claude[0].management, ExtensionManagement::Managed);
    assert!(!claude[0].enabled);
}

#[test]
#[serial_test::serial]
fn adopting_detected_mcp_keeps_live_bytes_and_returns_no_connection_payload() {
    use std::sync::Arc;

    let temp = tempfile::tempdir().expect("temp home");
    let _home = TestHome::set(temp.path());
    std::fs::create_dir_all(temp.path().join(".claude")).expect("create Claude directory");
    let live_path = temp.path().join(".claude.json");
    let original = br#"{"theme":"dark","mcpServers":{"browser":{"command":"npx","args":["secret-package"],"env":{"PRIVATE_TOKEN":"fixture-secret"}}}}"#;
    std::fs::write(&live_path, original).expect("write Claude MCP config");
    let state = crate::store::AppState::new(Arc::new(
        crate::database::Database::memory().expect("memory database"),
    ));
    let store = super::ExtensionStore { state };

    let refreshed = store
        .adopt_detected(ToolId::ClaudeCode, ExtensionKind::Mcp)
        .expect("adopt detected MCP");
    assert_eq!(refreshed.len(), 1);
    assert_eq!(refreshed[0].management, ExtensionManagement::Managed);
    assert!(refreshed[0].enabled);
    assert_eq!(
        std::fs::read(&live_path).expect("read untouched live config"),
        original
    );

    let rows = store
        .state
        .db
        .get_all_mcp_servers()
        .expect("read managed MCP rows");
    let stored = rows.get("browser").expect("browser was imported");
    assert!(stored.apps.claude);
    assert!(stored.server.to_string().contains("fixture-secret"));
    let wire = serde_json::to_string(&refreshed).expect("serialize product inventory");
    for forbidden in [
        "fixture-secret",
        "secret-package",
        "PRIVATE_TOKEN",
        "command",
        "args",
        "env",
    ] {
        assert!(!wire.contains(forbidden), "wire leaked {forbidden}");
    }

    let retry = store
        .adopt_detected(ToolId::ClaudeCode, ExtensionKind::Mcp)
        .expect("a completed adoption is idempotent");
    assert_eq!(retry, refreshed);
    assert_eq!(
        std::fs::read(&live_path).expect("read live config after retry"),
        original
    );
}

#[test]
fn prompts_cannot_enter_a_detected_adoption_flow() {
    use std::sync::Arc;

    let state = crate::store::AppState::new(Arc::new(
        crate::database::Database::memory().expect("memory database"),
    ));
    let store = super::ExtensionStore { state };
    let error = store
        .adopt_detected(ToolId::ClaudeCode, ExtensionKind::Prompt)
        .expect_err("Prompts have no detected inventory contract");
    assert_eq!(error.code, ErrorCode::ConfigWriteFailed);
    assert_eq!(error.message_key, "error.extension.adoptUnsupported");
}

fn local_draft() -> McpInstallDraft {
    McpInstallDraft {
        name: "  Files  ".to_string(),
        description: Some(" Read selected files. ".to_string()),
        connection: McpConnectionDraft::Stdio {
            command: "npx".to_string(),
            arguments: vec!["-y".to_string(), "server-files".to_string()],
        },
    }
}

#[test]
#[serial_test::serial]
fn guided_install_writes_the_real_upstream_db_and_live_config_then_verifies() {
    use std::sync::Arc;

    let temp = tempfile::tempdir().expect("temp home");
    let _home = TestHome::set(temp.path());
    std::fs::create_dir_all(temp.path().join(".claude")).expect("initialized Claude dir");
    std::fs::write(temp.path().join(".claude.json"), br#"{"theme":"dark"}"#)
        .expect("seed Claude config");
    let state = crate::store::AppState::new(Arc::new(
        crate::database::Database::memory().expect("memory database"),
    ));

    let installed = super::mcp::install(
        &state,
        scope(ToolId::ClaudeCode),
        &crate::app_config::AppType::Claude,
        "files-a1b2c3d4",
        &local_draft(),
    )
    .expect("install succeeds");
    assert_eq!(installed.id, "files-a1b2c3d4");
    assert_eq!(installed.name, "Files");
    assert!(installed.enabled);

    let rows = state.db.get_all_mcp_servers().expect("read MCP rows");
    let stored = rows.get("files-a1b2c3d4").expect("stored row");
    assert!(stored.apps.claude);
    assert_eq!(
        stored.server,
        super::mcp::connection_spec(&local_draft().connection)
    );

    let live: serde_json::Value = serde_json::from_slice(
        &std::fs::read(temp.path().join(".claude.json")).expect("read Claude config"),
    )
    .expect("valid Claude config");
    assert_eq!(live["theme"], "dark");
    let server = &live["mcpServers"]["files-a1b2c3d4"];
    // `claude_mcp::wrap_command_for_windows` rewrites the npx family into `cmd /c npx …`
    // there, because those launchers are batch files that `CreateProcess` cannot start.
    if cfg!(target_os = "windows") {
        assert_eq!(server["command"], "cmd");
        assert_eq!(
            server["args"],
            serde_json::json!(["/c", "npx", "-y", "server-files"])
        );
    } else {
        assert_eq!(server["command"], "npx");
        assert_eq!(server["args"], serde_json::json!(["-y", "server-files"]));
    }
    assert!(server.get("env").is_none());
}

#[test]
#[serial_test::serial]
fn failed_live_write_removes_the_new_db_row_and_preserves_source_bytes() {
    use std::sync::Arc;

    let temp = tempfile::tempdir().expect("temp home");
    let _home = TestHome::set(temp.path());
    std::fs::create_dir_all(temp.path().join(".claude")).expect("initialized Claude dir");
    let live_path = temp.path().join(".claude.json");
    let original = b"{ definitely not valid json";
    std::fs::write(&live_path, original).expect("seed corrupt config");
    let state = crate::store::AppState::new(Arc::new(
        crate::database::Database::memory().expect("memory database"),
    ));

    let error = super::mcp::install(
        &state,
        scope(ToolId::ClaudeCode),
        &crate::app_config::AppType::Claude,
        "files-f0e1d2c3",
        &local_draft(),
    )
    .expect_err("corrupt live config blocks install");
    assert_eq!(error.message_key, "error.mcp.installCleanupFailed");
    assert!(!state
        .db
        .get_all_mcp_servers()
        .expect("read MCP rows")
        .contains_key("files-f0e1d2c3"));
    assert_eq!(std::fs::read(&live_path).expect("read source"), original);
}

#[test]
#[serial_test::serial]
fn global_mcp_removal_updates_every_enabled_tool_and_verifies_the_row_is_gone() {
    use std::sync::Arc;

    let temp = tempfile::tempdir().expect("temp home");
    let _home = TestHome::set(temp.path());
    std::fs::create_dir_all(temp.path().join(".claude")).expect("initialized Claude dir");
    std::fs::create_dir_all(temp.path().join(".codex")).expect("initialized Codex dir");
    std::fs::write(temp.path().join(".claude.json"), br#"{"theme":"dark"}"#)
        .expect("seed Claude config");
    std::fs::write(
        temp.path().join(".codex/config.toml"),
        "model = \"gpt-5\"\n",
    )
    .expect("seed Codex config");
    let state = crate::store::AppState::new(Arc::new(
        crate::database::Database::memory().expect("memory database"),
    ));

    super::mcp::install(
        &state,
        scope(ToolId::Codex),
        &crate::app_config::AppType::Codex,
        "files-b1c2d3e4",
        &local_draft(),
    )
    .expect("install for Codex");
    super::mcp::set_enabled(
        &state,
        &crate::app_config::AppType::Claude,
        "files-b1c2d3e4",
        true,
    )
    .expect("enable for Claude");

    let resolved = super::mcp::installed(
        &state,
        scope(ToolId::Codex),
        &crate::app_config::AppType::Codex,
        "files-b1c2d3e4",
    )
    .expect("resolve authoritative target");
    assert_eq!(resolved.name, "Files");
    super::mcp::remove(&state, scope(ToolId::Codex), "files-b1c2d3e4")
        .expect("global removal succeeds");

    assert!(!state
        .db
        .get_all_mcp_servers()
        .expect("read MCP rows")
        .contains_key("files-b1c2d3e4"));
    let claude: serde_json::Value = serde_json::from_slice(
        &std::fs::read(temp.path().join(".claude.json")).expect("read Claude config"),
    )
    .expect("valid Claude config");
    assert_eq!(claude["theme"], "dark");
    assert!(claude["mcpServers"].get("files-b1c2d3e4").is_none());
    let codex =
        std::fs::read_to_string(temp.path().join(".codex/config.toml")).expect("read Codex config");
    assert!(codex.contains("model = \"gpt-5\""));
    assert!(!codex.contains("files-b1c2d3e4"));
}

#[test]
#[serial_test::serial]
fn failed_global_removal_restores_the_exact_row_and_preserves_corrupt_source_bytes() {
    use std::sync::Arc;

    let temp = tempfile::tempdir().expect("temp home");
    let _home = TestHome::set(temp.path());
    std::fs::create_dir_all(temp.path().join(".claude")).expect("initialized Claude dir");
    let live_path = temp.path().join(".claude.json");
    let original_bytes = b"{ definitely not valid json";
    std::fs::write(&live_path, original_bytes).expect("seed corrupt config");
    let state = crate::store::AppState::new(Arc::new(
        crate::database::Database::memory().expect("memory database"),
    ));
    let apps = crate::app_config::McpApps {
        claude: true,
        ..Default::default()
    };
    let original = crate::app_config::McpServer {
        id: "private-a1b2c3d4".to_string(),
        name: "Private connection".to_string(),
        server: serde_json::json!({
            "type": "stdio",
            "command": "private-server",
            "env": { "PRIVATE_TOKEN": "fixture-secret" }
        }),
        apps,
        description: Some("Keep this exact row".to_string()),
        homepage: Some("https://example.test".to_string()),
        docs: None,
        tags: vec!["fixture".to_string()],
    };
    state
        .db
        .save_mcp_server(&original)
        .expect("seed row without touching live config");

    let error = super::mcp::remove(&state, scope(ToolId::ClaudeCode), &original.id)
        .expect_err("corrupt live config blocks a confirmed removal");
    assert_eq!(error.message_key, "error.mcp.removeRestoreFailed");
    let restored = state
        .db
        .get_all_mcp_servers()
        .expect("read MCP rows")
        .shift_remove(&original.id)
        .expect("database row was restored");
    assert_eq!(restored.name, original.name);
    assert_eq!(restored.server, original.server);
    assert_eq!(restored.apps, original.apps);
    assert_eq!(restored.description, original.description);
    assert_eq!(restored.homepage, original.homepage);
    assert_eq!(restored.tags, original.tags);
    assert_eq!(
        std::fs::read(&live_path).expect("read corrupt source"),
        original_bytes
    );
    assert!(!error
        .technical_message
        .as_deref()
        .unwrap_or_default()
        .contains("fixture-secret"));
}

#[test]
fn missing_mcp_removal_target_is_a_stable_not_installed_error() {
    use std::sync::Arc;

    let state = crate::store::AppState::new(Arc::new(
        crate::database::Database::memory().expect("memory database"),
    ));
    let error = super::mcp::remove(&state, scope(ToolId::OpenCode), "missing")
        .expect_err("a missing row cannot be reported as removed");
    assert_eq!(error.code, ErrorCode::ExtensionNotFound);
    assert_eq!(error.message_key, "error.mcp.notInstalled");
}

#[test]
#[serial_test::serial]
#[cfg(any(target_os = "macos", target_os = "windows"))]
fn claude_desktop_is_a_real_mcp_scope_with_non_destructive_adoption_and_toggles() {
    use std::sync::Arc;

    let temp = tempfile::tempdir().expect("temp home");
    let _home = TestHome::set(temp.path());
    let live_path = crate::platform::claude_desktop_mcp_config_path()
        .expect("Claude Desktop has a config path on this platform");
    std::fs::create_dir_all(live_path.parent().expect("Claude Desktop config parent"))
        .expect("create Claude Desktop directory");
    let original = br#"{"deploymentMode":"3p","mcpServers":{"existing":{"command":"npx","args":["private-package"],"env":{"PRIVATE_TOKEN":"fixture-secret"}}}}"#;
    std::fs::write(&live_path, original).expect("seed Claude Desktop config");
    let state = crate::store::AppState::new(Arc::new(
        crate::database::Database::memory().expect("memory database"),
    ));
    let store = super::ExtensionStore { state };
    let desktop = ExtensionScope::desktop_app(DesktopAppId::ClaudeDesktop);

    let adopted = store
        .adopt_detected_scope(desktop, ExtensionKind::Mcp)
        .expect("adopt Claude Desktop MCP");
    assert_eq!(adopted.len(), 1);
    assert_eq!(adopted[0].scope, desktop);
    assert_eq!(adopted[0].management, ExtensionManagement::Managed);
    assert!(adopted[0].enabled);
    assert_eq!(
        std::fs::read(&live_path).expect("read untouched desktop config"),
        original
    );
    let wire = serde_json::to_string(&adopted).expect("serialize desktop inventory");
    for forbidden in [
        "fixture-secret",
        "private-package",
        "PRIVATE_TOKEN",
        "command",
    ] {
        assert!(!wire.contains(forbidden), "wire leaked {forbidden}");
    }

    let installed = store
        .install_mcp_in_scope(desktop, "files-c1d2e3f4", &local_draft())
        .expect("install Claude Desktop MCP");
    assert_eq!(installed.scope, desktop);
    assert!(installed.enabled);
    let after_install: serde_json::Value = serde_json::from_slice(
        &std::fs::read(&live_path).expect("read Claude Desktop config after install"),
    )
    .expect("parse Claude Desktop config after install");
    assert_eq!(after_install["deploymentMode"], "3p");
    assert_eq!(
        after_install["mcpServers"]["existing"]["env"]["PRIVATE_TOKEN"],
        "fixture-secret"
    );
    assert_eq!(
        after_install["mcpServers"]["files-c1d2e3f4"]["command"],
        "npx"
    );

    let disabled = store
        .set_enabled_scope(desktop, ExtensionKind::Mcp, "files-c1d2e3f4", false)
        .expect("disable Claude Desktop MCP");
    assert!(
        !disabled
            .iter()
            .find(|entry| entry.id == "files-c1d2e3f4")
            .expect("managed row remains visible")
            .enabled
    );
    let after_disable: serde_json::Value = serde_json::from_slice(
        &std::fs::read(&live_path).expect("read Claude Desktop config after disable"),
    )
    .expect("parse Claude Desktop config after disable");
    assert!(after_disable["mcpServers"].get("files-c1d2e3f4").is_none());
    assert!(after_disable["mcpServers"].get("existing").is_some());

    let enabled = store
        .set_enabled_scope(desktop, ExtensionKind::Mcp, "files-c1d2e3f4", true)
        .expect("re-enable Claude Desktop MCP");
    assert!(
        enabled
            .iter()
            .find(|entry| entry.id == "files-c1d2e3f4")
            .expect("managed row remains visible")
            .enabled
    );
}

#[test]
#[serial_test::serial]
#[cfg(any(target_os = "macos", target_os = "windows"))]
fn failed_claude_desktop_toggle_restores_the_exact_database_row_and_live_bytes() {
    use std::sync::Arc;

    let temp = tempfile::tempdir().expect("temp home");
    let _home = TestHome::set(temp.path());
    let live_path = crate::platform::claude_desktop_mcp_config_path()
        .expect("Claude Desktop has a config path on this platform");
    std::fs::create_dir_all(live_path.parent().expect("Claude Desktop config parent"))
        .expect("create Claude Desktop directory");
    let original_bytes = br#"{"mcpServers":[]}"#;
    std::fs::write(&live_path, original_bytes).expect("seed malformed desktop config");
    let state = crate::store::AppState::new(Arc::new(
        crate::database::Database::memory().expect("memory database"),
    ));
    let original = crate::app_config::McpServer {
        id: "private-d1e2f3a4".to_string(),
        name: "Private connection".to_string(),
        server: serde_json::json!({
            "command": "private-server",
            "env": { "PRIVATE_TOKEN": "fixture-secret" }
        }),
        apps: crate::app_config::McpApps {
            claude: true,
            claude_desktop: false,
            ..Default::default()
        },
        description: Some("Keep the exact row".to_string()),
        homepage: Some("https://example.test".to_string()),
        docs: Some("https://example.test/docs".to_string()),
        tags: vec!["fixture".to_string()],
    };
    state
        .db
        .save_mcp_server(&original)
        .expect("seed managed row without touching live config");

    let error = super::mcp::set_enabled(
        &state,
        &crate::app_config::AppType::ClaudeDesktop,
        &original.id,
        true,
    )
    .expect_err("malformed Claude Desktop config blocks toggle");
    assert_eq!(error.message_key, "error.extension.toggleFailed");
    assert_eq!(
        std::fs::read(&live_path).expect("read unchanged desktop config"),
        original_bytes
    );
    let restored = state
        .db
        .get_all_mcp_servers()
        .expect("read restored MCP rows")
        .shift_remove(&original.id)
        .expect("database row remains");
    assert_eq!(restored.name, original.name);
    assert_eq!(restored.server, original.server);
    assert_eq!(restored.apps, original.apps);
    assert_eq!(restored.description, original.description);
    assert_eq!(restored.homepage, original.homepage);
    assert_eq!(restored.docs, original.docs);
    assert_eq!(restored.tags, original.tags);
    assert!(!error
        .technical_message
        .as_deref()
        .unwrap_or_default()
        .contains("fixture-secret"));
}

#[test]
#[serial_test::serial]
fn copying_a_detected_skill_gives_the_other_tool_its_own_independent_folder() {
    use std::sync::Arc;

    let temp = tempfile::tempdir().expect("temp home");
    let _home = TestHome::set(temp.path());
    let source = temp.path().join(".claude").join("skills").join("unity-cli");
    std::fs::create_dir_all(&source).expect("create the Claude Skill directory");
    let body = b"---\nname: Unity CLI\ndescription: Drives the Unity editor\n---\n";
    std::fs::write(source.join("SKILL.md"), body).expect("write the Skill");
    let state = crate::store::AppState::new(Arc::new(
        crate::database::Database::memory().expect("memory database"),
    ));
    let store = super::ExtensionStore { state };

    let codex = store
        .copy_detected_skill(scope(ToolId::ClaudeCode), ToolId::Codex, "unity-cli")
        .expect("copy the detected Skill into Codex");
    assert_eq!(codex.len(), 1);
    assert_eq!(codex[0].id, "unity-cli");
    assert_eq!(codex[0].management, ExtensionManagement::Detected);

    let copy = temp.path().join(".codex").join("skills").join("unity-cli");
    assert_eq!(
        std::fs::read(copy.join("SKILL.md")).expect("Codex now has its own copy"),
        body
    );
    // A real folder, not a link: the copy must not follow the original.
    assert!(!std::fs::symlink_metadata(&copy)
        .expect("read the copy's metadata")
        .file_type()
        .is_symlink());
    assert_eq!(
        std::fs::read(source.join("SKILL.md")).expect("the original is untouched"),
        body
    );

    let wire = serde_json::to_string(&codex).expect("serialize the refreshed inventory");
    assert!(!wire.contains(".codex"));
    assert!(!wire.contains("SKILL.md"));
}

#[test]
#[serial_test::serial]
fn copying_over_an_existing_skill_is_refused_instead_of_overwriting_it() {
    use std::sync::Arc;

    let temp = tempfile::tempdir().expect("temp home");
    let _home = TestHome::set(temp.path());
    let source = temp.path().join(".claude").join("skills").join("unity-cli");
    std::fs::create_dir_all(&source).expect("create the Claude Skill directory");
    std::fs::write(source.join("SKILL.md"), b"---\nname: Unity CLI\n---\n")
        .expect("write the Skill");
    let occupied = temp.path().join(".codex").join("skills").join("unity-cli");
    std::fs::create_dir_all(&occupied).expect("create the Codex Skill directory");
    let theirs = b"---\nname: Unity CLI\ndescription: The user's own version\n---\n";
    std::fs::write(occupied.join("SKILL.md"), theirs).expect("write the Codex Skill");
    let state = crate::store::AppState::new(Arc::new(
        crate::database::Database::memory().expect("memory database"),
    ));
    let store = super::ExtensionStore { state };

    let error = store
        .copy_detected_skill(scope(ToolId::ClaudeCode), ToolId::Codex, "unity-cli")
        .expect_err("a same-named Skill must never be overwritten");
    assert_eq!(error.message_key, "error.extension.copyFailed");
    assert_eq!(
        std::fs::read(occupied.join("SKILL.md")).expect("their copy survived"),
        theirs
    );
}

#[test]
#[serial_test::serial]
#[cfg(unix)]
fn a_source_holding_a_symlink_is_refused_rather_than_silently_materialized() {
    use std::sync::Arc;

    let temp = tempfile::tempdir().expect("temp home");
    let _home = TestHome::set(temp.path());
    let outside = temp.path().join("outside.txt");
    std::fs::write(&outside, b"private").expect("write the outside file");
    let source = temp.path().join(".claude").join("skills").join("unity-cli");
    std::fs::create_dir_all(&source).expect("create the Claude Skill directory");
    std::fs::write(source.join("SKILL.md"), b"---\nname: Unity CLI\n---\n")
        .expect("write the Skill");
    std::os::unix::fs::symlink(&outside, source.join("notes.txt")).expect("plant the link");
    let state = crate::store::AppState::new(Arc::new(
        crate::database::Database::memory().expect("memory database"),
    ));
    let store = super::ExtensionStore { state };

    let error = store
        .copy_detected_skill(scope(ToolId::ClaudeCode), ToolId::Codex, "unity-cli")
        .expect_err("a third-party source with a link cannot be copied");
    assert_eq!(error.message_key, "error.extension.copyFailed");
    assert!(!temp
        .path()
        .join(".codex")
        .join("skills")
        .join("unity-cli")
        .exists());
}

#[test]
#[serial_test::serial]
fn a_target_that_cannot_hold_skills_is_refused_before_anything_is_read() {
    use std::sync::Arc;

    let temp = tempfile::tempdir().expect("temp home");
    let _home = TestHome::set(temp.path());
    let source = temp.path().join(".claude").join("skills").join("unity-cli");
    std::fs::create_dir_all(&source).expect("create the Claude Skill directory");
    std::fs::write(source.join("SKILL.md"), b"---\nname: Unity CLI\n---\n")
        .expect("write the Skill");
    let state = crate::store::AppState::new(Arc::new(
        crate::database::Database::memory().expect("memory database"),
    ));
    let store = super::ExtensionStore { state };

    // OpenClaw has no Skills capability, so the compatibility layer must not
    // be the thing that decides; see the application-layer gate.
    let refused = crate::application::extension_directory::supports(
        ExtensionKind::Skill,
        &crate::compat::ccswitch::tools::capabilities_for(ToolId::OpenClaw),
    );
    assert!(!refused, "the fixture tool unexpectedly manages Skills");

    // The store itself still refuses to invent a copy for a Skill it cannot find.
    let error = store
        .copy_detected_skill(scope(ToolId::ClaudeCode), ToolId::Codex, "not-here")
        .expect_err("an unknown Skill id has nothing to copy");
    assert_eq!(error.message_key, "error.extension.notFound");
}
