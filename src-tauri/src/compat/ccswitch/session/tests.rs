use std::path::PathBuf;

use super::{
    message_role, normalize_query, project_basename, reference_for, resume_command,
    resume_target_from_meta, reveal_target_from_meta, sanitize_body, shell_word, summary_from_meta,
    SessionResumeTarget,
};
use crate::domain::{ErrorCode, SessionMessageRole, ToolId};
use crate::session_manager::SessionMeta;

fn session(provider: &str) -> SessionMeta {
    SessionMeta {
        provider_id: provider.to_string(),
        session_id: "session-42".to_string(),
        title: Some(" Fix\n\0 tests ".to_string()),
        summary: Some("Investigate   the suite".to_string()),
        project_dir: Some("/Users/example/work/ai-manager/".to_string()),
        created_at: Some(1),
        last_active_at: Some(2),
        source_path: Some("/Users/example/.tool/session.jsonl".to_string()),
        resume_command: Some("renderer must not receive this".to_string()),
    }
}

#[test]
fn opaque_reference_is_stable_and_identity_bound() {
    let first = session("claude");
    let reference = reference_for(&first);
    assert_eq!(reference.len(), 64);
    assert!(reference.bytes().all(|byte| byte.is_ascii_hexdigit()));
    assert_eq!(reference_for(&first), reference);

    let mut different_path = first.clone();
    different_path.source_path = Some("/tmp/other.jsonl".to_string());
    assert_ne!(reference_for(&different_path), reference);
}

#[test]
fn summary_strips_paths_ids_commands_and_control_characters() {
    let summary = summary_from_meta(&session("claude")).expect("known provider");
    assert_eq!(summary.tool, ToolId::ClaudeCode);
    assert_eq!(summary.title.as_deref(), Some("Fix tests"));
    assert_eq!(summary.project_name.as_deref(), Some("ai-manager"));
    assert!(summary.resumable);
    let encoded = serde_json::to_string(&summary).expect("serialize summary");
    for forbidden in ["session-42", "/Users/", "resumeCommand", "sourcePath"] {
        assert!(!encoded.contains(forbidden), "summary leaked {forbidden}");
    }
}

#[test]
fn resume_arguments_are_rebuilt_from_the_closed_tool_registry() {
    let cases = [
        ("claude", ToolId::ClaudeCode, vec!["--resume", "session-42"]),
        ("codex", ToolId::Codex, vec!["resume", "session-42"]),
        ("opencode", ToolId::OpenCode, vec!["-s", "session-42"]),
        ("gemini", ToolId::GeminiCli, vec!["--resume", "session-42"]),
        (
            "grokbuild",
            ToolId::GrokBuild,
            vec!["--resume", "session-42"],
        ),
    ];
    for (provider, tool, expected) in cases {
        let target = resume_target_from_meta(&session(provider)).expect("resume target");
        assert_eq!(target.tool, tool);
        assert_eq!(target.args, expected);
    }

    let pi = resume_target_from_meta(&session("pi")).expect("Pi target");
    assert_eq!(pi.tool, ToolId::Pi);
    assert_eq!(
        pi.args,
        vec!["--session", "/Users/example/.tool/session.jsonl"]
    );

    for provider in ["openclaw", "hermes"] {
        let error = resume_target_from_meta(&session(provider)).expect_err("not resumable");
        assert_eq!(error.code, ErrorCode::LaunchFailed);
        assert_eq!(error.message_key, "error.session.resumeUnsupported");
    }
}

#[test]
fn reveal_target_resolves_the_source_path_or_fails_closed() {
    let target = reveal_target_from_meta(&session("claude")).expect("reveal target");
    assert_eq!(target, PathBuf::from("/Users/example/.tool/session.jsonl"));

    let mut missing_path = session("claude");
    missing_path.source_path = None;
    let error = reveal_target_from_meta(&missing_path).expect_err("no source path");
    assert_eq!(error.code, ErrorCode::SessionNotFound);
    assert_eq!(error.message_key, "error.session.notFound");
}

#[test]
fn query_and_content_bounds_are_enforced_without_breaking_utf8() {
    assert_eq!(
        normalize_query(Some("  CoDeX ")).unwrap().as_deref(),
        Some("codex")
    );
    let error = normalize_query(Some(&"x".repeat(201))).expect_err("long query");
    assert_eq!(error.code, ErrorCode::ConfigParseFailed);

    let (body, truncated) = sanitize_body("ab🙂cd", 5);
    assert_eq!(body, "ab");
    assert!(truncated);
    let (body, truncated) = sanitize_body("a\0b\r\nc", 32);
    assert_eq!(body, "ab\n\nc");
    assert!(!truncated);
}

#[test]
fn provider_and_path_helpers_are_closed_and_cross_platform() {
    assert_eq!(project_basename(r"C:\\work\\project\\"), Some("project"));
    assert_eq!(project_basename("/work/project/"), Some("project"));
    assert_eq!(message_role("FUNCTION"), SessionMessageRole::Tool);
    assert_eq!(message_role("alien"), SessionMessageRole::Other);
    assert!(summary_from_meta(&session("unknown")).is_none());
}

#[cfg(unix)]
#[test]
fn manual_resume_command_is_shell_safe_and_mirrors_the_rebuilt_argv() {
    let target = SessionResumeTarget {
        tool: ToolId::Codex,
        args: vec!["resume".to_string(), "01a084d9-cfdc".to_string()],
        working_directory: PathBuf::from("/work/it's here"),
    };
    assert_eq!(
        resume_command(&target),
        r"cd '/work/it'\''s here' && codex resume 01a084d9-cfdc"
    );

    // Ordinary paths and session ids must not be smeared with quotes — the user has to be able to read and paste them at a glance.
    assert_eq!(
        shell_word("/Users/example/work/ai-manager"),
        "/Users/example/work/ai-manager"
    );
    assert_eq!(shell_word("a b"), "'a b'");
    assert_eq!(shell_word(""), "''");
}
