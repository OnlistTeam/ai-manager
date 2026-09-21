//! End-to-end integration tests for project profile snapshot/apply.
//!
//! A full apply writes live config files; support.rs points HOME at a temp dir, so it is safe.

use std::fs;

use serde_json::json;

use ai_manager_lib::{
    AppType, InstalledSkill, McpServer, McpService, ProfilePayload, ProfileScope, ProfileService,
    Prompt, PromptService, Provider, ProviderService, SkillApps, SkillService,
};

#[path = "support.rs"]
mod support;
use support::{create_test_state, ensure_test_home, reset_test_fs, test_mutex};

fn claude_provider(id: &str, token: &str) -> Provider {
    Provider::with_id(
        id.to_string(),
        id.to_uppercase(),
        json!({
            "env": {
                "ANTHROPIC_AUTH_TOKEN": token,
                "ANTHROPIC_BASE_URL": "https://api.test"
            }
        }),
        None,
    )
}

/// Claude Desktop provider: without meta it defaults to Direct mode, which only requires a token + base_url in env.
fn desktop_provider(id: &str, token: &str) -> Provider {
    Provider::with_id(
        id.to_string(),
        id.to_uppercase(),
        json!({
            "env": {
                "ANTHROPIC_AUTH_TOKEN": token,
                "ANTHROPIC_BASE_URL": "https://desktop.test"
            }
        }),
        None,
    )
}

fn mcp_server(id: &str, claude_enabled: bool) -> McpServer {
    serde_json::from_value(json!({
        "id": id,
        "name": id,
        "server": { "command": "echo", "args": [] },
        "apps": { "claude": claude_enabled }
    }))
    .expect("construct mcp server")
}

fn prompt(id: &str, enabled: bool) -> Prompt {
    Prompt {
        id: id.to_string(),
        name: id.to_uppercase(),
        content: format!("# prompt {id}\n"),
        description: None,
        enabled,
        created_at: Some(1_000),
        updated_at: Some(1_000),
    }
}

fn installed_skill(id: &str, directory: &str, claude_enabled: bool) -> InstalledSkill {
    InstalledSkill {
        id: id.to_string(),
        name: id.to_string(),
        description: None,
        directory: directory.to_string(),
        repo_owner: None,
        repo_name: None,
        repo_branch: None,
        readme_url: None,
        apps: SkillApps {
            claude: claude_enabled,
            ..Default::default()
        },
        installed_at: 1_000,
        content_hash: None,
        updated_at: 0,
    }
}

fn write_ssot_skill(directory: &str) {
    let dir = SkillService::get_ssot_dir()
        .expect("resolve skills SSOT dir")
        .join(directory);
    fs::create_dir_all(&dir).expect("create skill dir");
    fs::write(
        dir.join("SKILL.md"),
        format!("---\nname: {directory}\ndescription: Test skill\n---\n"),
    )
    .expect("write SKILL.md");
}

#[test]
fn profile_snapshot_apply_roundtrip_restores_configuration() {
    let _guard = test_mutex().lock().expect("acquire test mutex");
    reset_test_fs();
    let home = ensure_test_home();

    let state = create_test_state().expect("create test state");

    // ---- Seed: 2 Claude providers (p1 current) + 2 MCPs + 1 skill + 2 prompts ----
    state
        .db
        .save_provider(AppType::Claude.as_str(), &claude_provider("p1", "key-1"))
        .expect("save provider p1");
    state
        .db
        .save_provider(AppType::Claude.as_str(), &claude_provider("p2", "key-2"))
        .expect("save provider p2");
    state
        .db
        .set_current_provider(AppType::Claude.as_str(), "p1")
        .expect("set current provider p1");

    // Claude Desktop has only one active dimension, the provider (MCP/skills/prompts do not apply).
    state
        .db
        .save_provider(
            AppType::ClaudeDesktop.as_str(),
            &desktop_provider("d1", "dk-1"),
        )
        .expect("save desktop provider d1");
    state
        .db
        .save_provider(
            AppType::ClaudeDesktop.as_str(),
            &desktop_provider("d2", "dk-2"),
        )
        .expect("save desktop provider d2");
    state
        .db
        .set_current_provider(AppType::ClaudeDesktop.as_str(), "d1")
        .expect("set current desktop provider d1");

    // Keep the live settings.json in sync with p1 (needed by the switch_normal backfill).
    let claude_dir = home.join(".claude");
    fs::create_dir_all(&claude_dir).expect("create .claude dir");
    fs::write(
        claude_dir.join("settings.json"),
        serde_json::to_string_pretty(&claude_provider("p1", "key-1").settings_config)
            .expect("serialize p1 settings"),
    )
    .expect("seed live settings.json");

    state
        .db
        .save_mcp_server(&mcp_server("m1", true))
        .expect("save mcp m1");
    state
        .db
        .save_mcp_server(&mcp_server("m2", false))
        .expect("save mcp m2");

    write_ssot_skill("test-skill");
    state
        .db
        .save_skill(&installed_skill("local:test-skill", "test-skill", true))
        .expect("save skill");

    state
        .db
        .save_prompt(AppType::Claude.as_str(), &prompt("pr1", true))
        .expect("save prompt pr1");
    state
        .db
        .save_prompt(AppType::Claude.as_str(), &prompt("pr2", false))
        .expect("save prompt pr2");

    // ---- Save project A (created on the Claude tab: snapshots only the Claude state) ----
    let profile_a = ProfileService::create(&state, "Project A", ProfileScope::Claude)
        .expect("create profile A");
    let payload: ProfilePayload =
        serde_json::from_str(&profile_a.payload).expect("parse profile A payload");
    assert_eq!(payload.providers.claude.as_deref(), Some("p1"));
    assert_eq!(payload.mcp.claude, Some(vec!["m1".to_string()]));
    assert_eq!(
        payload.skills.claude,
        Some(vec!["local:test-skill".to_string()])
    );
    assert_eq!(payload.prompts.claude.as_deref(), Some("pr1"));
    assert_eq!(
        payload.providers.codex, None,
        "codex side not captured when creating from the claude group"
    );
    assert_eq!(payload.mcp.codex, None, "uncaptured side stays None");
    assert_eq!(
        payload.providers.claude_desktop, None,
        "claude desktop has its own profile scope"
    );

    // ---- Change all four config kinds (through the real switch paths) ----
    ProviderService::switch(&state, AppType::Claude, "p2").expect("switch to p2");
    // Desktop now has its own profile scope; a Claude-scope apply must not affect Desktop.
    #[cfg(any(target_os = "macos", windows))]
    ProviderService::switch(&state, AppType::ClaudeDesktop, "d2").expect("switch desktop to d2");
    McpService::toggle_app(&state, "m1", AppType::Claude, false).expect("disable m1");
    McpService::toggle_app(&state, "m2", AppType::Claude, true).expect("enable m2");
    SkillService::toggle_app(&state.db, "local:test-skill", &AppType::Claude, false)
        .expect("disable skill");
    PromptService::enable_prompt(&state, AppType::Claude, "pr2").expect("enable pr2");

    // ---- Apply project A (Claude scope): restores only the Claude side ----
    let (warnings, _) = ProfileService::apply(&state, &profile_a.id, ProfileScope::Claude)
        .expect("apply profile A");
    assert!(warnings.is_empty(), "unexpected warnings: {warnings:?}");

    let current = state
        .db
        .get_current_provider(AppType::Claude.as_str())
        .expect("get current provider");
    assert_eq!(current.as_deref(), Some("p1"), "provider restored to p1");

    // The Claude scope no longer manages Desktop: after apply, Desktop keeps the state it had
    // before. On macOS/Windows it was switched to d2 above; on Linux (CI) Desktop switching is
    // unsupported and that line is compiled out by cfg, so Desktop is still the seeded d1.
    // Either way this verifies a claude-scope apply does not touch Desktop.
    let current_desktop = state
        .db
        .get_current_provider(AppType::ClaudeDesktop.as_str())
        .expect("get current desktop provider");
    #[cfg(any(target_os = "macos", windows))]
    let expected_desktop = "d2";
    #[cfg(not(any(target_os = "macos", windows)))]
    let expected_desktop = "d1";
    assert_eq!(
        current_desktop.as_deref(),
        Some(expected_desktop),
        "desktop provider untouched by claude-scope apply"
    );

    let servers = state.db.get_all_mcp_servers().expect("get mcp servers");
    assert!(servers.get("m1").expect("m1").apps.claude, "m1 re-enabled");
    assert!(!servers.get("m2").expect("m2").apps.claude, "m2 disabled");

    let skills = state.db.get_all_installed_skills().expect("get skills");
    assert!(
        skills.get("local:test-skill").expect("skill").apps.claude,
        "skill re-enabled"
    );

    let prompts = state
        .db
        .get_prompts(AppType::Claude.as_str())
        .expect("get prompts");
    assert!(prompts.get("pr1").expect("pr1").enabled, "pr1 re-enabled");
    assert!(!prompts.get("pr2").expect("pr2").enabled, "pr2 disabled");

    let live_prompt = fs::read_to_string(claude_dir.join("CLAUDE.md")).expect("read CLAUDE.md");
    assert_eq!(
        live_prompt,
        prompt("pr1", true).content,
        "live memory file restored"
    );

    assert_eq!(
        state
            .db
            .get_current_profile_id("claude")
            .expect("get current profile id")
            .as_deref(),
        Some(profile_a.id.as_str()),
        "profile A marked as current for claude scope"
    );
    assert_eq!(
        state
            .db
            .get_current_profile_id("codex")
            .expect("get codex current profile id"),
        None,
        "codex scope marker untouched by claude-group apply"
    );
}

#[test]
fn shared_profile_sides_are_isolated_and_mergeable() {
    let _guard = test_mutex().lock().expect("acquire test mutex");
    reset_test_fs();
    let home = ensure_test_home();

    let state = create_test_state().expect("create test state");

    // Seed: the Claude side has a current provider plus an enabled MCP.
    state
        .db
        .save_provider(AppType::Claude.as_str(), &claude_provider("p1", "key-1"))
        .expect("save provider p1");
    state
        .db
        .set_current_provider(AppType::Claude.as_str(), "p1")
        .expect("set current provider p1");
    let claude_dir = home.join(".claude");
    fs::create_dir_all(&claude_dir).expect("create .claude dir");
    fs::write(
        claude_dir.join("settings.json"),
        serde_json::to_string_pretty(&claude_provider("p1", "key-1").settings_config)
            .expect("serialize p1 settings"),
    )
    .expect("seed live settings.json");
    state
        .db
        .save_mcp_server(&mcp_server("m1", true))
        .expect("save mcp m1");

    // Create a project on the Codex tab: the snapshot must not capture any Claude-side state.
    let project = ProfileService::create(&state, "Shared Project", ProfileScope::Codex)
        .expect("create project from codex tab");
    let payload: ProfilePayload =
        serde_json::from_str(&project.payload).expect("parse project payload");
    assert_eq!(
        payload.providers.claude, None,
        "claude slot not captured by codex-side snapshot"
    );
    assert_eq!(payload.mcp.claude, None);
    assert_eq!(payload.providers.claude_desktop, None);
    assert_eq!(payload.mcp.codex, Some(vec![]), "codex side captured");

    // Apply with Codex scope: only the codex current marker moves, the Claude side is untouched.
    let (warnings, _) = ProfileService::apply(&state, &project.id, ProfileScope::Codex)
        .expect("apply project on codex side");
    assert!(warnings.is_empty(), "unexpected warnings: {warnings:?}");

    assert_eq!(
        state
            .db
            .get_current_provider(AppType::Claude.as_str())
            .expect("get claude current provider")
            .as_deref(),
        Some("p1"),
        "claude provider untouched"
    );
    let servers = state.db.get_all_mcp_servers().expect("get mcp servers");
    assert!(
        servers.get("m1").expect("m1").apps.claude,
        "claude MCP untouched"
    );
    assert_eq!(
        state
            .db
            .get_current_profile_id("codex")
            .expect("get codex current profile id")
            .as_deref(),
        Some(project.id.as_str())
    );
    assert_eq!(
        state
            .db
            .get_current_profile_id("claude")
            .expect("get claude current profile id"),
        None,
        "claude scope marker untouched by codex-side apply"
    );

    // Applying the same shared project on the Claude tab: that side was never snapshotted, so leave the config alone, mark it current, and return a hint.
    let (warnings, _) = ProfileService::apply(&state, &project.id, ProfileScope::Claude)
        .expect("apply project on claude side");
    assert_eq!(warnings.len(), 1, "uncaptured side yields one hint");
    assert!(warnings[0].contains("no claude configuration captured"));
    let servers = state.db.get_all_mcp_servers().expect("get mcp servers");
    assert!(
        servers.get("m1").expect("m1").apps.claude,
        "claude MCP still untouched by uncaptured apply"
    );
    assert_eq!(
        state
            .db
            .get_current_profile_id("claude")
            .expect("get claude current profile id")
            .as_deref(),
        Some(project.id.as_str()),
        "claude side now bound to the shared project"
    );

    // "Update with current state" on the Claude tab: snapshot the claude side, keep the codex snapshot as is.
    let updated =
        ProfileService::update(&state, &project.id, None, true, Some(ProfileScope::Claude))
            .expect("resnapshot claude side");
    let payload: ProfilePayload =
        serde_json::from_str(&updated.payload).expect("parse updated payload");
    assert_eq!(payload.providers.claude.as_deref(), Some("p1"));
    assert_eq!(payload.mcp.claude, Some(vec!["m1".to_string()]));
    assert_eq!(
        payload.mcp.codex,
        Some(vec![]),
        "codex side snapshot preserved by claude-side resnapshot"
    );
}

#[test]
fn profile_apply_reports_dangling_references_and_continues() {
    let _guard = test_mutex().lock().expect("acquire test mutex");
    reset_test_fs();
    let _home = ensure_test_home();

    let state = create_test_state().expect("create test state");

    state
        .db
        .save_mcp_server(&mcp_server("m1", false))
        .expect("save mcp m1");

    // Hand-build a payload referencing resources that do not exist.
    let payload = json!({
        "providers": { "claude": "ghost-provider" },
        "mcp": { "claude": ["m1", "ghost-mcp"] },
        "skills": { "claude": ["ghost-skill"] },
        "prompts": { "claude": "ghost-prompt" }
    });
    let profile = ai_manager_lib::Profile {
        id: "dangling-test".to_string(),
        name: "Dangling".to_string(),
        payload: payload.to_string(),
        sort_order: None,
        created_at: Some(1_000),
        updated_at: Some(1_000),
    };
    state.db.save_profile(&profile).expect("save profile");

    let (warnings, _) = ProfileService::apply(&state, "dangling-test", ProfileScope::Claude)
        .expect("apply succeeds");
    assert_eq!(
        warnings.len(),
        4,
        "each dangling reference yields one warning: {warnings:?}"
    );

    // Valid entries still take effect: m1 gets enabled.
    let servers = state.db.get_all_mcp_servers().expect("get mcp servers");
    assert!(
        servers.get("m1").expect("m1").apps.claude,
        "m1 enabled despite warnings"
    );

    // After the best-effort run it is still marked as the scope's current project.
    assert_eq!(
        state
            .db
            .get_current_profile_id("claude")
            .expect("get current profile id")
            .as_deref(),
        Some("dangling-test")
    );
}

#[test]
fn clear_current_profile_only_clears_scoped_marker() {
    let _guard = test_mutex().lock().expect("acquire test mutex");
    reset_test_fs();
    let _home = ensure_test_home();

    let state = create_test_state().expect("create test state");

    state
        .db
        .set_current_profile_id("claude", Some("claude-profile"))
        .expect("set claude current profile");
    state
        .db
        .set_current_profile_id("codex", Some("codex-profile"))
        .expect("set codex current profile");

    // Clearing the claude scope must not affect the codex scope.
    state
        .db
        .set_current_profile_id("claude", None)
        .expect("clear claude current profile");
    assert_eq!(
        state
            .db
            .get_current_profile_id("claude")
            .expect("get claude current profile id"),
        None
    );
    assert_eq!(
        state
            .db
            .get_current_profile_id("codex")
            .expect("get codex current profile id")
            .as_deref(),
        Some("codex-profile")
    );
}

#[test]
fn switching_profile_autosaves_previous_profile_state() {
    let _guard = test_mutex().lock().expect("acquire test mutex");
    reset_test_fs();
    let home = ensure_test_home();

    let state = create_test_state().expect("create test state");

    // ---- Seed: two Claude-side providers / MCPs / prompts ----
    state
        .db
        .save_provider(AppType::Claude.as_str(), &claude_provider("p1", "key-1"))
        .expect("save provider p1");
    state
        .db
        .save_provider(AppType::Claude.as_str(), &claude_provider("p2", "key-2"))
        .expect("save provider p2");
    state
        .db
        .set_current_provider(AppType::Claude.as_str(), "p1")
        .expect("set current provider p1");

    let claude_dir = home.join(".claude");
    fs::create_dir_all(&claude_dir).expect("create .claude dir");
    fs::write(
        claude_dir.join("settings.json"),
        serde_json::to_string_pretty(&claude_provider("p1", "key-1").settings_config)
            .expect("serialize p1 settings"),
    )
    .expect("seed live settings.json");

    state
        .db
        .save_mcp_server(&mcp_server("m1", true))
        .expect("save mcp m1");
    state
        .db
        .save_mcp_server(&mcp_server("m2", false))
        .expect("save mcp m2");

    state
        .db
        .save_prompt(AppType::Claude.as_str(), &prompt("pr1", true))
        .expect("save prompt pr1");
    state
        .db
        .save_prompt(AppType::Claude.as_str(), &prompt("pr2", false))
        .expect("save prompt pr2");

    // ---- Project A: state X (p1 / m1 / pr1) ----
    let project_a = ProfileService::create(&state, "Project A", ProfileScope::Claude)
        .expect("create project A");
    let (warnings, _) = ProfileService::apply(&state, &project_a.id, ProfileScope::Claude)
        .expect("apply project A");
    assert!(warnings.is_empty(), "unexpected warnings: {warnings:?}");

    // ---- Under A, move to state Y (p2 / m2 / pr2), then create Project B from it ----
    ProviderService::switch(&state, AppType::Claude, "p2").expect("switch to p2");
    McpService::toggle_app(&state, "m1", AppType::Claude, false).expect("disable m1");
    McpService::toggle_app(&state, "m2", AppType::Claude, true).expect("enable m2");
    PromptService::enable_prompt(&state, AppType::Claude, "pr2").expect("enable pr2");

    let project_b = ProfileService::create(&state, "Project B", ProfileScope::Claude)
        .expect("create project B");

    // ---- Switch from A to B: auto-save current state Y into A, then load B's Y ----
    let (warnings, _) = ProfileService::apply(&state, &project_b.id, ProfileScope::Claude)
        .expect("switch to project B");
    assert!(warnings.is_empty(), "unexpected warnings: {warnings:?}");

    assert_eq!(
        state
            .db
            .get_current_provider(AppType::Claude.as_str())
            .expect("get current provider")
            .as_deref(),
        Some("p2"),
        "provider switched to p2"
    );
    let servers = state.db.get_all_mcp_servers().expect("get mcp servers");
    assert!(!servers.get("m1").expect("m1").apps.claude, "m1 disabled");
    assert!(servers.get("m2").expect("m2").apps.claude, "m2 enabled");
    let prompts = state
        .db
        .get_prompts(AppType::Claude.as_str())
        .expect("get prompts");
    assert!(!prompts.get("pr1").expect("pr1").enabled, "pr1 disabled");
    assert!(prompts.get("pr2").expect("pr2").enabled, "pr2 enabled");

    // Project A was auto-saved as state Y, the state it was left in.
    let saved_a = state
        .db
        .get_profile(&project_a.id)
        .expect("get project A")
        .expect("project A exists");
    let payload_a: ProfilePayload =
        serde_json::from_str(&saved_a.payload).expect("parse project A payload");
    assert_eq!(payload_a.providers.claude.as_deref(), Some("p2"));
    assert_eq!(payload_a.mcp.claude, Some(vec!["m2".to_string()]));
    assert_eq!(payload_a.prompts.claude.as_deref(), Some("pr2"));

    // ---- Under B, go back to state X, then switch back to A ----
    ProviderService::switch(&state, AppType::Claude, "p1").expect("switch to p1");
    McpService::toggle_app(&state, "m1", AppType::Claude, true).expect("enable m1");
    McpService::toggle_app(&state, "m2", AppType::Claude, false).expect("disable m2");
    PromptService::enable_prompt(&state, AppType::Claude, "pr1").expect("enable pr1");

    let (warnings, _) = ProfileService::apply(&state, &project_a.id, ProfileScope::Claude)
        .expect("switch back to project A");
    assert!(warnings.is_empty(), "unexpected warnings: {warnings:?}");

    // Switching back to A: auto-save B as state X first, then load A's last-left state Y.
    assert_eq!(
        state
            .db
            .get_current_provider(AppType::Claude.as_str())
            .expect("get current provider")
            .as_deref(),
        Some("p2"),
        "project A restored to the state when we left it (p2)"
    );
    let servers = state.db.get_all_mcp_servers().expect("get mcp servers");
    assert!(
        !servers.get("m1").expect("m1").apps.claude,
        "m1 stays disabled"
    );
    assert!(
        servers.get("m2").expect("m2").apps.claude,
        "m2 stays enabled"
    );
    let prompts = state
        .db
        .get_prompts(AppType::Claude.as_str())
        .expect("get prompts");
    assert!(
        !prompts.get("pr1").expect("pr1").enabled,
        "pr1 stays disabled"
    );
    assert!(
        prompts.get("pr2").expect("pr2").enabled,
        "pr2 stays enabled"
    );

    // Project B was auto-saved as state X, the state it was left in.
    let saved_b = state
        .db
        .get_profile(&project_b.id)
        .expect("get project B")
        .expect("project B exists");
    let payload_b: ProfilePayload =
        serde_json::from_str(&saved_b.payload).expect("parse project B payload");
    assert_eq!(payload_b.providers.claude.as_deref(), Some("p1"));
    assert_eq!(payload_b.mcp.claude, Some(vec!["m1".to_string()]));
    assert_eq!(payload_b.prompts.claude.as_deref(), Some("pr1"));
}

#[test]
fn profile_switch_auto_disables_takeover_before_apply() {
    let _guard = test_mutex().lock().expect("acquire test mutex");
    reset_test_fs();
    let home = ensure_test_home();

    let state = create_test_state().expect("create test state");

    // Use an ephemeral port to avoid clashes on the test machine.
    futures::executor::block_on(async {
        let mut proxy_config = state.db.get_proxy_config().await.expect("get proxy config");
        proxy_config.listen_port = 0;
        state
            .db
            .update_proxy_config(proxy_config)
            .await
            .expect("set ephemeral proxy port");
    });

    // ---- Two Claude providers: custom1 and custom2 ----
    let mut custom1 = claude_provider("custom1", "custom-key-1");
    custom1.category = Some("custom".to_string());
    state
        .db
        .save_provider(AppType::Claude.as_str(), &custom1)
        .expect("save custom1 provider");

    let mut custom2 = claude_provider("custom2", "custom-key-2");
    custom2.category = Some("custom".to_string());
    state
        .db
        .save_provider(AppType::Claude.as_str(), &custom2)
        .expect("save custom2 provider");

    // Initial state: custom1 with proxy takeover on.
    ProviderService::switch(&state, AppType::Claude, "custom1").expect("switch to custom1");
    let rt = tokio::runtime::Runtime::new().expect("create tokio runtime");
    rt.block_on(state.proxy_service.set_takeover_for_app("claude", true))
        .expect("enable claude takeover");

    let (proxy_enabled_before, _) = state.db.get_proxy_flags_sync("claude");
    assert!(
        proxy_enabled_before,
        "takeover should be active before apply"
    );

    // ---- Build a project snapshot that targets custom2 ----
    let project = ProfileService::create(&state, "Custom2 Project", ProfileScope::Claude)
        .expect("create project");
    let mut project = state
        .db
        .get_profile(&project.id)
        .expect("get project")
        .expect("project exists");
    let mut payload: ProfilePayload =
        serde_json::from_str(&project.payload).expect("parse project payload");
    payload.providers.claude = Some("custom2".to_string());
    project.payload = serde_json::to_string(&payload).expect("serialize payload");
    state
        .db
        .save_profile(&project)
        .expect("save updated project");

    // ---- Apply the project: takeover must be turned off unconditionally, then switch to custom2 ----
    let (warnings, _) = ProfileService::apply(&state, &project.id, ProfileScope::Claude)
        .expect("apply custom2 project");
    assert!(
        warnings.is_empty(),
        "switching project should not warn: {warnings:?}"
    );

    // Takeover is off.
    let (proxy_enabled_after, _) = state.db.get_proxy_flags_sync("claude");
    assert!(
        !proxy_enabled_after,
        "proxy takeover should be auto-disabled before applying profile"
    );

    // The current provider switched to custom2.
    assert_eq!(
        state
            .db
            .get_current_provider(AppType::Claude.as_str())
            .expect("get current provider")
            .as_deref(),
        Some("custom2"),
        "current provider should be custom2"
    );

    // The live config must point at custom2's real endpoint, not the proxy address.
    let settings_path = home.join(".claude/settings.json");
    let settings: serde_json::Value =
        serde_json::from_str(&fs::read_to_string(&settings_path).expect("read settings"))
            .expect("parse settings");
    let base_url = settings
        .get("env")
        .and_then(|e| e.get("ANTHROPIC_BASE_URL"))
        .and_then(|v| v.as_str());
    assert_eq!(
        base_url,
        Some("https://api.test"),
        "live config should point to real endpoint after auto-disable"
    );
}

#[cfg(any(target_os = "macos", windows))]
#[test]
fn claude_desktop_profile_scope_is_independent() {
    let _guard = test_mutex().lock().expect("acquire test mutex");
    reset_test_fs();
    let _home = ensure_test_home();

    let state = create_test_state().expect("create test state");

    state
        .db
        .save_provider(
            AppType::ClaudeDesktop.as_str(),
            &desktop_provider("d1", "dk-1"),
        )
        .expect("save desktop provider d1");
    state
        .db
        .save_provider(
            AppType::ClaudeDesktop.as_str(),
            &desktop_provider("d2", "dk-2"),
        )
        .expect("save desktop provider d2");
    state
        .db
        .set_current_provider(AppType::ClaudeDesktop.as_str(), "d1")
        .expect("set current desktop provider d1");

    // Create a project on the Desktop tab: snapshots only the Desktop provider.
    let project = ProfileService::create(&state, "Desktop Project", ProfileScope::ClaudeDesktop)
        .expect("create desktop profile");
    let payload: ProfilePayload =
        serde_json::from_str(&project.payload).expect("parse desktop payload");
    assert_eq!(payload.providers.claude_desktop.as_deref(), Some("d1"));
    assert_eq!(payload.providers.claude, None, "claude slot untouched");
    assert_eq!(payload.providers.codex, None, "codex slot untouched");

    // Switch to d2.
    ProviderService::switch(&state, AppType::ClaudeDesktop, "d2").expect("switch desktop to d2");

    // Apply the Desktop project: restores d1.
    let (warnings, _) = ProfileService::apply(&state, &project.id, ProfileScope::ClaudeDesktop)
        .expect("apply desktop profile");
    assert!(warnings.is_empty(), "unexpected warnings: {warnings:?}");

    assert_eq!(
        state
            .db
            .get_current_provider(AppType::ClaudeDesktop.as_str())
            .expect("get current desktop provider")
            .as_deref(),
        Some("d1"),
        "desktop provider restored by desktop-scope apply"
    );
    assert_eq!(
        state
            .db
            .get_current_profile_id(ProfileScope::ClaudeDesktop.as_str())
            .expect("get desktop current profile id")
            .as_deref(),
        Some(project.id.as_str()),
        "desktop scope marker set"
    );
}
