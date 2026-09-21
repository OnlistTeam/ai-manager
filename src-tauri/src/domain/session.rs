use serde::{Deserialize, Serialize};

use super::ToolId;

/// Privacy-preserving session row exposed to the product renderer. The stable
/// reference is opaque: browsing a list of sessions never needs a provider
/// session id or a local path, so the list does not carry one.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct SessionSummary {
    pub reference: String,
    pub tool: ToolId,
    pub title: Option<String>,
    pub preview: Option<String>,
    pub project_name: Option<String>,
    pub created_at: Option<i64>,
    pub last_active_at: Option<i64>,
    pub resumable: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct SessionList {
    pub items: Vec<SessionSummary>,
    pub total_count: u32,
    pub limited: bool,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum SessionMessageRole {
    User,
    Assistant,
    System,
    Tool,
    Other,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct SessionMessage {
    pub role: SessionMessageRole,
    pub content: String,
    pub timestamp: Option<i64>,
    pub truncated: bool,
}

/// What a user gets after explicitly opening one session. Unlike the list,
/// this carries the two facts they need when a terminal handoff goes wrong:
/// where the session runs and how to resume it by hand. Both are read-only
/// for the renderer — resuming still travels back as the opaque reference,
/// and the backend rebuilds argv from its own tool registry.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct SessionThread {
    pub reference: String,
    pub messages: Vec<SessionMessage>,
    pub total_count: u32,
    pub limited: bool,
    pub working_directory: Option<String>,
    pub resume_command: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::{SessionList, SessionMessage, SessionMessageRole, SessionSummary, SessionThread};
    use crate::domain::ToolId;

    #[test]
    fn product_session_wire_format_is_opaque_and_path_free() {
        let summary = SessionSummary {
            reference: "a".repeat(64),
            tool: ToolId::ClaudeCode,
            title: Some("Fix tests".to_string()),
            preview: Some("Please inspect the suite".to_string()),
            project_name: Some("ai-manager".to_string()),
            created_at: Some(1),
            last_active_at: Some(2),
            resumable: true,
        };
        let value = serde_json::to_value(SessionList {
            items: vec![summary],
            total_count: 1,
            limited: false,
        })
        .expect("serialize session list");
        let encoded = value.to_string();
        assert_eq!(value["items"][0]["tool"], "claude-code");
        for forbidden in ["sessionId", "sourcePath", "resumeCommand", "/Users/"] {
            assert!(!encoded.contains(forbidden), "payload leaked {forbidden}");
        }
    }

    #[test]
    fn thread_roles_and_truncation_are_explicit() {
        let value = serde_json::to_value(SessionThread {
            reference: "b".repeat(64),
            messages: vec![SessionMessage {
                role: SessionMessageRole::Assistant,
                content: "Done".to_string(),
                timestamp: Some(42),
                truncated: true,
            }],
            total_count: 2,
            limited: true,
            working_directory: Some("/work/project".to_string()),
            resume_command: Some("cd /work/project && codex resume abc".to_string()),
        })
        .expect("serialize thread");
        assert_eq!(value["messages"][0]["role"], "assistant");
        assert_eq!(value["messages"][0]["truncated"], true);
        assert_eq!(value["totalCount"], 2);
        // These two are only available once the user explicitly opens a session — they are still absent from the list.
        assert_eq!(value["workingDirectory"], "/work/project");
        assert_eq!(
            value["resumeCommand"],
            "cd /work/project && codex resume abc"
        );
    }
}
