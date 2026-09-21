//! Product-owned Prompt management models.
//!
//! The list view continues to use the small `Extension` projection. Prompt
//! content crosses the product boundary only when the user explicitly opens
//! one item for editing. Drafts are inbound-only and redact their content from
//! `Debug`, so standing instructions cannot leak through routine diagnostics.

use serde::{Deserialize, Serialize};

use super::{AppError, ErrorCode, ToolId};

pub const MAX_PROMPT_NAME_CHARS: usize = 80;
pub const MAX_PROMPT_DESCRIPTION_CHARS: usize = 500;
pub const MAX_PROMPT_CONTENT_BYTES: usize = 1024 * 1024;

#[derive(Clone, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct PromptDraft {
    pub name: String,
    pub description: Option<String>,
    pub content: String,
}

impl std::fmt::Debug for PromptDraft {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("PromptDraft")
            .field("name", &self.name)
            .field("description", &self.description)
            .field("content", &"***")
            .finish()
    }
}

impl PromptDraft {
    pub fn validate(&self) -> Result<(), AppError> {
        let name = self.name.trim();
        if name.is_empty() {
            return Err(invalid("error.prompt.nameRequired", "Prompt name is empty"));
        }
        if name.chars().count() > MAX_PROMPT_NAME_CHARS {
            return Err(invalid(
                "error.prompt.nameTooLong",
                "Prompt name is too long",
            ));
        }
        if has_forbidden_single_line_character(name) {
            return Err(invalid(
                "error.prompt.nameInvalid",
                "Prompt name contains control characters",
            ));
        }

        if let Some(description) = self.description.as_deref() {
            let description = description.trim();
            if description.chars().count() > MAX_PROMPT_DESCRIPTION_CHARS {
                return Err(invalid(
                    "error.prompt.descriptionTooLong",
                    "Prompt description is too long",
                ));
            }
            if has_forbidden_single_line_character(description) {
                return Err(invalid(
                    "error.prompt.descriptionInvalid",
                    "Prompt description contains control characters",
                ));
            }
        }

        if self.content.trim().is_empty() {
            return Err(invalid(
                "error.prompt.contentRequired",
                "Prompt content is empty",
            ));
        }
        if self.content.len() > MAX_PROMPT_CONTENT_BYTES {
            return Err(invalid(
                "error.prompt.contentTooLong",
                "Prompt content is too large",
            ));
        }
        if self.content.contains('\0') {
            return Err(invalid(
                "error.prompt.contentInvalid",
                "Prompt content contains NUL",
            ));
        }

        Ok(())
    }

    pub fn normalized_name(&self) -> String {
        self.name.trim().to_string()
    }

    pub fn normalized_description(&self) -> Option<String> {
        let normalized = self
            .description
            .as_deref()?
            .split_whitespace()
            .collect::<Vec<_>>()
            .join(" ");
        (!normalized.is_empty()).then_some(normalized)
    }

    pub fn normalized_content(&self) -> String {
        self.content.trim_end().to_string()
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PromptDetail {
    pub id: String,
    pub tool: ToolId,
    pub name: String,
    pub description: Option<String>,
    pub content: String,
    pub enabled: bool,
}

fn has_forbidden_single_line_character(value: &str) -> bool {
    value.chars().any(char::is_control)
}

fn invalid(message_key: &'static str, technical: &'static str) -> AppError {
    AppError::new(ErrorCode::ConfigWriteFailed, message_key)
        .with_technical(technical)
        .with_remediation("error.remediation.checkPromptSettings")
}

#[cfg(test)]
mod tests {
    use super::{PromptDetail, PromptDraft, MAX_PROMPT_CONTENT_BYTES};
    use crate::domain::ToolId;

    fn draft() -> PromptDraft {
        PromptDraft {
            name: "  Team rules  ".to_string(),
            description: Some("  Shared   coding rules.  ".to_string()),
            content: "# Rules\n\nRun tests.\n\n".to_string(),
        }
    }

    #[test]
    fn a_prompt_draft_is_inbound_only_normalized_and_redacted_in_debug() {
        let draft = draft();
        draft.validate().expect("valid Prompt draft");
        assert_eq!(draft.normalized_name(), "Team rules");
        assert_eq!(
            draft.normalized_description().as_deref(),
            Some("Shared coding rules.")
        );
        assert_eq!(draft.normalized_content(), "# Rules\n\nRun tests.");
        assert!(!format!("{draft:?}").contains("Run tests"));

        let unknown = serde_json::from_str::<PromptDraft>(
            r#"{"name":"Rules","description":null,"content":"Safe","path":"/private/file"}"#,
        );
        assert!(unknown.is_err(), "paths cannot enter the Prompt draft");
    }

    #[test]
    fn empty_nul_and_oversized_content_are_rejected_with_stable_keys() {
        let mut empty = draft();
        empty.content = " \n ".to_string();
        assert_eq!(
            empty.validate().expect_err("empty content").message_key,
            "error.prompt.contentRequired"
        );

        let mut nul = draft();
        nul.content = "safe\0hidden".to_string();
        assert_eq!(
            nul.validate().expect_err("NUL content").message_key,
            "error.prompt.contentInvalid"
        );

        let mut oversized = draft();
        oversized.content = "x".repeat(MAX_PROMPT_CONTENT_BYTES + 1);
        assert_eq!(
            oversized
                .validate()
                .expect_err("oversized content")
                .message_key,
            "error.prompt.contentTooLong"
        );
    }

    #[test]
    fn explicit_detail_has_content_but_no_path_or_timestamp() {
        let detail = PromptDetail {
            id: "prompt-1".to_string(),
            tool: ToolId::ClaudeCode,
            name: "Team rules".to_string(),
            description: None,
            content: "Private standing instructions".to_string(),
            enabled: false,
        };
        let wire = serde_json::to_string(&detail).expect("serialize Prompt detail");
        assert!(wire.contains("Private standing instructions"));
        for forbidden in ["path", "createdAt", "updatedAt"] {
            assert!(!wire.contains(forbidden));
        }
    }
}
