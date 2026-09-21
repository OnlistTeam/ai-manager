use std::sync::Arc;

use serial_test::serial;

use super::{
    import_current, list, remove, restore_row_or_escalate, restore_rows_or_escalate, save,
    set_enabled, LiveSnapshot,
};
use crate::app_config::AppType;
use crate::domain::{AppError, ErrorCode, PromptDraft, ToolId};
use crate::prompt::Prompt;
use crate::store::AppState;

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

struct TestEnvironmentVariable {
    name: &'static str,
    previous: Option<std::ffi::OsString>,
}

impl TestEnvironmentVariable {
    fn set(name: &'static str, value: &std::path::Path) -> Self {
        let previous = std::env::var_os(name);
        std::env::set_var(name, value);
        Self { name, previous }
    }
}

impl Drop for TestEnvironmentVariable {
    fn drop(&mut self) {
        match self.previous.take() {
            Some(value) => std::env::set_var(self.name, value),
            None => std::env::remove_var(self.name),
        }
    }
}

fn state() -> AppState {
    AppState::new(Arc::new(
        crate::database::Database::memory().expect("memory database"),
    ))
}

fn draft(name: &str, content: &str) -> PromptDraft {
    PromptDraft {
        name: name.to_string(),
        description: Some("Managed in AI Manager".to_string()),
        content: content.to_string(),
    }
}

fn stored(id: &str, content: &str, enabled: bool) -> Prompt {
    Prompt {
        id: id.to_string(),
        name: "Team rules".to_string(),
        content: content.to_string(),
        description: Some("Original description".to_string()),
        enabled,
        created_at: Some(10),
        updated_at: Some(20),
    }
}

#[test]
#[serial]
fn creating_an_inactive_prompt_never_rewrites_existing_live_instructions() {
    let temp = tempfile::tempdir().expect("temp home");
    let _home = TestHome::set(temp.path());
    let claude_dir = temp.path().join(".claude");
    std::fs::create_dir_all(&claude_dir).expect("Claude directory");
    let live_path = claude_dir.join("CLAUDE.md");
    let original = b"# Existing\n\nDo not touch this file.\n";
    std::fs::write(&live_path, original).expect("seed live Prompt");
    let state = state();

    let refreshed = save(
        &state,
        ToolId::ClaudeCode,
        &AppType::Claude,
        None,
        &draft(" New rules ", "# New\n\nRun tests.\n"),
    )
    .expect("create inactive Prompt");

    assert_eq!(refreshed.len(), 1);
    assert!(!refreshed[0].enabled);
    assert_eq!(
        std::fs::read(&live_path).expect("read untouched live Prompt"),
        original
    );
    let wire = serde_json::to_string(&refreshed).expect("serialize small inventory");
    assert!(!wire.contains("Run tests"));
    assert!(!wire.contains("content"));
}

#[test]
#[serial]
fn editing_the_active_prompt_backs_up_then_atomically_replaces_and_verifies() {
    let temp = tempfile::tempdir().expect("temp home");
    let _home = TestHome::set(temp.path());
    let claude_dir = temp.path().join(".claude");
    std::fs::create_dir_all(&claude_dir).expect("Claude directory");
    let live_path = claude_dir.join("CLAUDE.md");
    let original_live = b"# Original live\n";
    std::fs::write(&live_path, original_live).expect("seed live Prompt");
    let state = state();
    state
        .db
        .save_prompt(
            AppType::Claude.as_str(),
            &stored("active", "# Original live", true),
        )
        .expect("seed active row");

    let refreshed = save(
        &state,
        ToolId::ClaudeCode,
        &AppType::Claude,
        Some("active"),
        &draft("Updated rules", "# Updated\n\nRun every test.\n"),
    )
    .expect("edit active Prompt");

    assert_eq!(refreshed.len(), 1);
    assert!(refreshed[0].enabled);
    assert_eq!(
        std::fs::read_to_string(&live_path).expect("read updated live Prompt"),
        "# Updated\n\nRun every test."
    );
    let backups = std::fs::read_dir(claude_dir.join(".ai-manager-backups"))
        .expect("timestamped backup directory")
        .collect::<Result<Vec<_>, _>>()
        .expect("backup entries");
    assert_eq!(backups.len(), 1);
    assert_eq!(
        std::fs::read(backups[0].path()).expect("read backup"),
        original_live
    );
    assert!(backups[0]
        .file_name()
        .to_string_lossy()
        .starts_with("CLAUDE.md."));
}

/// Every switch or edit of the active Prompt backs up the live file; the
/// directory is pruned to the configured retain count like the workspace's.
#[test]
#[serial]
fn live_prompt_backups_are_pruned_to_the_retain_count() {
    let temp = tempfile::tempdir().expect("temp home");
    let _home = TestHome::set(temp.path());
    let claude_dir = temp.path().join(".claude");
    std::fs::create_dir_all(&claude_dir).expect("Claude directory");
    std::fs::write(claude_dir.join("CLAUDE.md"), b"# Original live\n").expect("seed live Prompt");
    let state = state();
    state
        .db
        .save_prompt(
            AppType::Claude.as_str(),
            &stored("active", "# Original live", true),
        )
        .expect("seed active row");

    let retain = crate::settings::effective_backup_retain_count();
    for round in 0..retain + 2 {
        save(
            &state,
            ToolId::ClaudeCode,
            &AppType::Claude,
            Some("active"),
            &draft("Updated rules", &format!("# Round {round}\n")),
        )
        .expect("edit active Prompt");
    }

    let backups = std::fs::read_dir(claude_dir.join(".ai-manager-backups"))
        .expect("timestamped backup directory")
        .collect::<Result<Vec<_>, _>>()
        .expect("backup entries");
    assert_eq!(backups.len(), retain);
}

#[test]
#[serial]
fn rollback_restores_the_exact_database_row_and_live_bytes() {
    let temp = tempfile::tempdir().expect("temp home");
    let _home = TestHome::set(temp.path());
    let live_path = temp.path().join(".claude/CLAUDE.md");
    std::fs::create_dir_all(live_path.parent().expect("parent")).expect("Claude directory");
    let original_bytes = b"# Original\r\nKeep CRLF.\r\n".to_vec();
    std::fs::write(&live_path, b"# Partially changed\n").expect("seed changed live Prompt");
    let state = state();
    let original = stored("active", "# Original\r\nKeep CRLF.", true);
    let attempted = stored("active", "# Partially changed", true);
    state
        .db
        .save_prompt(AppType::Claude.as_str(), &attempted)
        .expect("seed attempted row");

    let returned = restore_row_or_escalate(
        &state,
        &AppType::Claude,
        Some(&original),
        &attempted,
        Some((&live_path, &LiveSnapshot::Present(original_bytes.clone()))),
        AppError::new(ErrorCode::ConfigWriteFailed, "error.prompt.saveFailed")
            .with_technical("injected post-write failure"),
    );

    assert_eq!(returned.message_key, "error.prompt.saveFailed");
    let restored = state
        .db
        .get_prompts(AppType::Claude.as_str())
        .expect("read restored rows")
        .shift_remove("active")
        .expect("restored row");
    assert_eq!(restored.name, original.name);
    assert_eq!(restored.content, original.content);
    assert_eq!(restored.description, original.description);
    assert_eq!(restored.enabled, original.enabled);
    assert_eq!(restored.created_at, original.created_at);
    assert_eq!(restored.updated_at, original.updated_at);
    assert_eq!(
        std::fs::read(live_path).expect("read restored live"),
        original_bytes
    );
}

#[test]
#[serial]
fn active_removal_is_rejected_and_inactive_removal_does_not_touch_live() {
    let temp = tempfile::tempdir().expect("temp home");
    let _home = TestHome::set(temp.path());
    let live_path = temp.path().join(".claude/CLAUDE.md");
    std::fs::create_dir_all(live_path.parent().expect("parent")).expect("Claude directory");
    let live = b"# Active live\n";
    std::fs::write(&live_path, live).expect("seed live Prompt");
    let state = state();
    for prompt in [
        stored("active", "# Active live", true),
        stored("draft", "# Draft", false),
    ] {
        state
            .db
            .save_prompt(AppType::Claude.as_str(), &prompt)
            .expect("seed Prompt row");
    }

    let error = remove(&state, ToolId::ClaudeCode, &AppType::Claude, "active")
        .expect_err("active Prompt cannot be removed");
    assert_eq!(error.message_key, "error.prompt.removeActive");

    let remaining = remove(&state, ToolId::ClaudeCode, &AppType::Claude, "draft")
        .expect("remove inactive Prompt");
    assert_eq!(remaining.len(), 1);
    assert_eq!(remaining[0].id, "active");
    assert_eq!(std::fs::read(live_path).expect("read live Prompt"), live);
}

#[test]
#[serial]
fn importing_current_instructions_is_idempotent_and_preserves_live_bytes() {
    let temp = tempfile::tempdir().expect("temp home");
    let _home = TestHome::set(temp.path());
    let live_path = temp.path().join(".claude/CLAUDE.md");
    std::fs::create_dir_all(live_path.parent().expect("parent")).expect("Claude directory");
    let live = b"# Existing manual instructions\n\nKeep these.\n";
    std::fs::write(&live_path, live).expect("seed live Prompt");
    let state = state();

    let first = import_current(&state, ToolId::ClaudeCode, &AppType::Claude)
        .expect("import current Prompt");
    let second =
        import_current(&state, ToolId::ClaudeCode, &AppType::Claude).expect("idempotent import");
    assert_eq!(first, second);
    assert_eq!(second.len(), 1);
    assert!(!second[0].enabled);
    assert_eq!(std::fs::read(live_path).expect("read unchanged live"), live);
    assert_eq!(
        list(&state, ToolId::ClaudeCode, &AppType::Claude)
            .expect("list Prompt")
            .len(),
        1
    );
}

#[test]
#[serial]
fn missing_and_empty_live_instructions_have_distinct_stable_errors() {
    let temp = tempfile::tempdir().expect("temp home");
    let _home = TestHome::set(temp.path());
    let state = state();
    let missing = import_current(&state, ToolId::ClaudeCode, &AppType::Claude)
        .expect_err("missing live Prompt");
    assert_eq!(missing.message_key, "error.prompt.importMissing");

    let live_path = temp.path().join(".claude/CLAUDE.md");
    std::fs::create_dir_all(live_path.parent().expect("parent")).expect("Claude directory");
    std::fs::write(&live_path, " \n ").expect("seed empty live Prompt");
    let empty = import_current(&state, ToolId::ClaudeCode, &AppType::Claude)
        .expect_err("empty live Prompt");
    assert_eq!(empty.message_key, "error.prompt.importEmpty");
}

#[test]
#[serial]
fn switching_the_active_prompt_never_backfills_live_edits_into_the_old_row() {
    let temp = tempfile::tempdir().expect("temp home");
    let _home = TestHome::set(temp.path());
    let claude_dir = temp.path().join(".claude");
    std::fs::create_dir_all(&claude_dir).expect("Claude directory");
    let live_path = claude_dir.join("CLAUDE.md");
    let hand_edited = b"# Edited by hand since the last switch\n";
    std::fs::write(&live_path, hand_edited).expect("seed hand-edited live Prompt");
    let state = state();
    for prompt in [
        stored("active", "# Original stored rules", true),
        stored("other", "# Other rules", false),
    ] {
        state
            .db
            .save_prompt(AppType::Claude.as_str(), &prompt)
            .expect("seed Prompt row");
    }

    set_enabled(&state, ToolId::ClaudeCode, &AppType::Claude, "other", true)
        .expect("switch to the other Prompt");

    assert_eq!(
        std::fs::read_to_string(&live_path).expect("read switched live Prompt"),
        "# Other rules"
    );
    let rows = state
        .db
        .get_prompts(AppType::Claude.as_str())
        .expect("read rows after switch");
    assert_eq!(rows.len(), 2, "switching must not invent rows: {rows:?}");
    let previous = rows.get("active").expect("previous row");
    assert!(!previous.enabled);
    assert_eq!(
        previous.content, "# Original stored rules",
        "the live edits were written back into the previously active row"
    );
    assert!(rows.get("other").expect("target row").enabled);
    let backups = std::fs::read_dir(claude_dir.join(".ai-manager-backups"))
        .expect("timestamped backup directory")
        .collect::<Result<Vec<_>, _>>()
        .expect("backup entries");
    assert_eq!(backups.len(), 1);
    assert_eq!(
        std::fs::read(backups[0].path()).expect("read backup"),
        hand_edited
    );
}

#[test]
#[serial]
fn every_standard_prompt_scope_switches_its_native_instruction_file() {
    let temp = tempfile::tempdir().expect("temp home");
    let _home = TestHome::set(temp.path());
    let _hermes_home = TestEnvironmentVariable::set("HERMES_HOME", &temp.path().join(".hermes"));
    let cases = [
        (ToolId::ClaudeCode, AppType::Claude, "CLAUDE.md"),
        (ToolId::Codex, AppType::Codex, "AGENTS.md"),
        (ToolId::GeminiCli, AppType::Gemini, "GEMINI.md"),
        (ToolId::GrokBuild, AppType::GrokBuild, "AGENTS.md"),
        (ToolId::OpenCode, AppType::OpenCode, "AGENTS.md"),
        (ToolId::OpenClaw, AppType::OpenClaw, "AGENTS.md"),
        (ToolId::Hermes, AppType::Hermes, "SOUL.md"),
    ];

    for (tool, app, expected_file) in cases {
        let live_path = crate::prompt_files::prompt_file_path(&app).expect("native Prompt path");
        assert_eq!(
            live_path.file_name().and_then(|name| name.to_str()),
            Some(expected_file),
            "{tool:?}"
        );
        std::fs::create_dir_all(live_path.parent().expect("Prompt parent"))
            .expect("create Prompt parent");
        std::fs::write(&live_path, format!("# Hand edit for {tool:?}"))
            .expect("seed native Prompt file");
        let state = state();
        for prompt in [
            stored("active", "# Previously managed", true),
            stored("target", &format!("# Managed by {tool:?}"), false),
        ] {
            state
                .db
                .save_prompt(app.as_str(), &prompt)
                .expect("seed Prompt row");
        }

        set_enabled(&state, tool, &app, "target", true).expect("switch native Prompt");

        assert_eq!(
            std::fs::read_to_string(&live_path).expect("read switched Prompt"),
            format!("# Managed by {tool:?}"),
            "{tool:?}"
        );
        let rows = state
            .db
            .get_prompts(app.as_str())
            .expect("read switched Prompt rows");
        assert!(!rows.get("active").expect("old Prompt").enabled, "{tool:?}");
        assert!(rows.get("target").expect("new Prompt").enabled, "{tool:?}");
        let backups = std::fs::read_dir(
            live_path
                .parent()
                .expect("Prompt parent")
                .join(".ai-manager-backups"),
        )
        .expect("Prompt backup directory")
        .collect::<Result<Vec<_>, _>>()
        .expect("Prompt backup entries");
        assert_eq!(backups.len(), 1, "{tool:?}");
    }
}

#[test]
#[serial]
fn switching_to_a_missing_prompt_is_a_stable_not_found() {
    let temp = tempfile::tempdir().expect("temp home");
    let _home = TestHome::set(temp.path());
    let state = state();
    let error = set_enabled(&state, ToolId::ClaudeCode, &AppType::Claude, "ghost", true)
        .expect_err("unknown Prompt cannot become active");
    assert_eq!(error.code, ErrorCode::ExtensionNotFound);
    assert_eq!(error.message_key, "error.prompt.notFound");
}

#[test]
#[serial]
fn switching_to_the_active_prompt_leaves_hand_edits_alone() {
    let temp = tempfile::tempdir().expect("temp home");
    let _home = TestHome::set(temp.path());
    let live_path = temp.path().join(".claude/CLAUDE.md");
    std::fs::create_dir_all(live_path.parent().expect("parent")).expect("Claude directory");
    let hand_edited = b"# Edited by hand\n";
    std::fs::write(&live_path, hand_edited).expect("seed hand-edited live Prompt");
    let state = state();
    state
        .db
        .save_prompt(
            AppType::Claude.as_str(),
            &stored("active", "# Stored rules", true),
        )
        .expect("seed active row");

    set_enabled(&state, ToolId::ClaudeCode, &AppType::Claude, "active", true)
        .expect("re-selecting the active Prompt is a no-op");

    assert_eq!(
        std::fs::read(&live_path).expect("read live Prompt"),
        hand_edited
    );
    assert!(!temp.path().join(".claude/.ai-manager-backups").exists());
}

#[test]
#[serial]
fn pi_switch_uses_native_agents_file_and_preserves_unmatched_live_content() {
    let _agent = crate::pi_config::test_support::TestAgentDir::new();
    let agent_dir = crate::pi_config::get_pi_agent_dir().expect("Pi agent directory");
    std::fs::create_dir_all(&agent_dir).expect("create Pi agent directory");
    let live_path = agent_dir.join("AGENTS.md");
    std::fs::write(&live_path, "# Hand-written Pi instructions\n").expect("seed native Pi Prompt");
    let state = state();
    state
        .db
        .save_prompt(
            AppType::Pi.as_str(),
            &stored("managed", "# Managed Pi instructions", false),
        )
        .expect("seed Pi Prompt row");

    set_enabled(&state, ToolId::Pi, &AppType::Pi, "managed", true).expect("switch Pi Prompt");

    assert_eq!(
        std::fs::read_to_string(&live_path).expect("read Pi AGENTS.md"),
        "# Managed Pi instructions"
    );
    let projected = list(&state, ToolId::Pi, &AppType::Pi).expect("list Pi Prompts");
    assert!(projected
        .iter()
        .any(|prompt| prompt.id == "managed" && prompt.enabled));
    let rows = state
        .db
        .get_prompts(AppType::Pi.as_str())
        .expect("read Pi Prompt rows");
    assert!(rows
        .values()
        .any(|prompt| { prompt.content == "# Hand-written Pi instructions\n" && !prompt.enabled }));
    assert!(rows.values().all(|prompt| !prompt.enabled));
}

#[test]
#[serial]
fn editing_active_pi_prompt_keeps_native_state_and_writes_a_recovery_copy() {
    let _agent = crate::pi_config::test_support::TestAgentDir::new();
    let agent_dir = crate::pi_config::get_pi_agent_dir().expect("Pi agent directory");
    std::fs::create_dir_all(&agent_dir).expect("create Pi agent directory");
    let live_path = agent_dir.join("AGENTS.md");
    std::fs::write(&live_path, "# Original Pi rules").expect("seed active native Pi Prompt");
    let state = state();
    state
        .db
        .save_prompt(
            AppType::Pi.as_str(),
            &stored("active", "# Original Pi rules", false),
        )
        .expect("seed Pi Prompt row");

    let refreshed = save(
        &state,
        ToolId::Pi,
        &AppType::Pi,
        Some("active"),
        &draft("Updated Pi rules", "# Updated Pi rules"),
    )
    .expect("edit active Pi Prompt");

    assert_eq!(
        std::fs::read_to_string(&live_path).expect("read updated Pi AGENTS.md"),
        "# Updated Pi rules"
    );
    assert!(refreshed
        .iter()
        .any(|prompt| prompt.id == "active" && prompt.enabled));
    let raw = state
        .db
        .get_prompts(AppType::Pi.as_str())
        .expect("read raw Pi Prompt row");
    assert!(!raw.get("active").expect("active Pi row").enabled);
    assert_eq!(
        raw.get("active").expect("active Pi row").content,
        "# Updated Pi rules"
    );
    let backups = std::fs::read_dir(agent_dir.join(".ai-manager-backups"))
        .expect("Pi recovery directory")
        .collect::<Result<Vec<_>, _>>()
        .expect("Pi recovery entries");
    assert_eq!(backups.len(), 1);
    assert_eq!(
        std::fs::read_to_string(backups[0].path()).expect("read Pi recovery copy"),
        "# Original Pi rules"
    );
}

#[test]
#[serial]
fn a_failed_switch_restores_every_touched_row_and_the_live_bytes() {
    let temp = tempfile::tempdir().expect("temp home");
    let _home = TestHome::set(temp.path());
    let live_path = temp.path().join(".claude/CLAUDE.md");
    std::fs::create_dir_all(live_path.parent().expect("parent")).expect("Claude directory");
    let original_bytes = b"# Live before the switch\n".to_vec();
    std::fs::write(&live_path, b"# Other rules").expect("seed half-switched live Prompt");
    let state = state();
    let previously_active = stored("active", "# Original stored rules", true);
    let target = stored("other", "# Other rules", false);
    // The half-finished switch: rows already flipped, live already replaced.
    for prompt in [
        Prompt {
            enabled: false,
            ..previously_active.clone()
        },
        Prompt {
            enabled: true,
            ..target.clone()
        },
    ] {
        state
            .db
            .save_prompt(AppType::Claude.as_str(), &prompt)
            .expect("seed flipped row");
    }

    let returned = restore_rows_or_escalate(
        &state,
        &AppType::Claude,
        &[previously_active.clone(), target.clone()],
        (&live_path, &LiveSnapshot::Present(original_bytes.clone())),
        AppError::new(
            ErrorCode::ConfigWriteFailed,
            "error.prompt.saveVerifyFailed",
        )
        .with_technical("injected post-write failure"),
    );

    assert_eq!(returned.message_key, "error.prompt.saveVerifyFailed");
    let rows = state
        .db
        .get_prompts(AppType::Claude.as_str())
        .expect("read restored rows");
    let restored_active = rows.get("active").expect("restored previously active row");
    assert!(restored_active.enabled);
    assert_eq!(restored_active.content, previously_active.content);
    let restored_target = rows.get("other").expect("restored target row");
    assert!(!restored_target.enabled);
    assert_eq!(restored_target.updated_at, target.updated_at);
    assert_eq!(
        std::fs::read(live_path).expect("read restored live"),
        original_bytes
    );
}
